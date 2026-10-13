mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "current_thread")]
async fn operator_can_read_space_sign_in_status_without_issuing_or_disclosing_a_key() {
    let f = Fixture::new().await;
    let path = format!("/admin/v0/spaces/{}/owner-credential", f.sid());
    let before = call(&f.app, "GET", &path, ADMIN, json!({})).await;
    assert_eq!(before["version"], 0);
    assert_eq!(before["active"], false);
    let issued = call(&f.app, "POST", &path, ADMIN, json!({"expected_version":0})).await;
    let status = call(&f.app, "GET", &path, ADMIN, json!({})).await;
    assert_eq!(status["version"], 1);
    assert_eq!(status["active"], true);
    assert_eq!(status.as_object().unwrap().len(), 3);
    assert!(
        !status
            .to_string()
            .contains(issued["credential"].as_str().unwrap())
    );
    assert_eq!(
        raw(
            &f.app,
            "GET",
            &path,
            issued["credential"].as_str().unwrap(),
            json!({})
        )
        .await
        .0,
        404
    );
    call(&f.app, "POST", &path, ADMIN, json!({"expected_version":1})).await;
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/v0/registry",
            issued["credential"].as_str().unwrap(),
            json!({"kind":"spaces"})
        )
        .await
        .0,
        401
    );
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_connection_default_delivers_to_primary_without_exposing_owner_inbox() {
    let f = Fixture::new().await;
    let root = format!("/admin/v0/spaces/{}", f.sid());
    let sender = call(
        &f.app,
        "POST",
        &format!("{root}/actors"),
        ADMIN,
        json!({"request_id":"sender","display_name":"Connected assistant","kind":"agent"}),
    )
    .await;
    let consumer = call(&f.app,"POST",&format!("{root}/consumers"),ADMIN,json!({"request_id":"sender-consumer","actor_id":sender["actor_id"],"view":"inbox","filter":{}})).await;
    let issued = call(&f.app,"POST",&format!("{root}/tokens"),ADMIN,json!({"request_id":"sender-connection","actor_id":sender["actor_id"],"consumer_id":consumer["consumer_id"],"scopes":["message.send"],"resource_grants":{"recipient_ids":[f.writer["actor_id"]]},"default_recipient_id":f.writer["actor_id"]})).await;
    let token = issued["token"].as_str().unwrap();
    let describe = call(&f.app, "GET", "/v0/describe", token, json!({})).await;
    assert_eq!(describe["default_recipient_id"], f.writer["actor_id"]);
    call(&f.app,"POST","/v0/messages",token,json!({"request_id":"handoff","kind":"info","subject":"Booking confirmed","body":{"text":"Friday, 19:00, two seats"},"metadata":{}})).await;
    assert_eq!(f.writer_inbox().await["items"].as_array().unwrap().len(), 1);
    assert!(f.inbox().await["items"].as_array().unwrap().is_empty());
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/v0/inbox/read",
            token,
            json!({"consumer_id":consumer["consumer_id"]})
        )
        .await
        .0,
        403
    );
    assert_eq!(raw(&f.app,"POST",&format!("{root}/tokens"),ADMIN,json!({"request_id":"ungranted-default","actor_id":sender["actor_id"],"consumer_id":consumer["consumer_id"],"scopes":["message.send"],"resource_grants":{},"default_recipient_id":f.writer["actor_id"]})).await.0,400);
    let old_default = call(&f.app, "GET", "/v0/describe", &f.writer_token, json!({})).await;
    assert_eq!(
        old_default["default_recipient_id"],
        f.space["default_actor_id"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn retry_keeps_the_first_reserved_recipient_after_changing_connection_default() {
    let f = Fixture::new().await;
    let path = format!("/admin/v0/spaces/{}/tokens", f.sid());
    let mut body = json!({"request_id":"primary-default","actor_id":f.writer["actor_id"],"consumer_id":f.writer_consumer["consumer_id"],"scopes":["message.send"],"resource_grants":{"recipient_ids":[f.writer["actor_id"],f.space["default_actor_id"]]},"default_recipient_id":f.writer["actor_id"]});
    let first = call(&f.app, "POST", &path, ADMIN, body.clone()).await;
    let message = json!({"request_id":"durable-default","kind":"info","subject":"Result","body":{"text":"Keep the original destination"}});
    let original = call(
        &f.app,
        "POST",
        "/v0/messages",
        first["token"].as_str().unwrap(),
        message.clone(),
    )
    .await;
    body["request_id"] = json!("owner-default");
    body["default_recipient_id"] = f.space["default_actor_id"].clone();
    let replacement = call(&f.app, "POST", &path, ADMIN, body).await;
    assert_eq!(
        call(
            &f.app,
            "POST",
            "/v0/messages",
            replacement["token"].as_str().unwrap(),
            message
        )
        .await,
        original
    );
    assert_eq!(f.writer_inbox().await["items"].as_array().unwrap().len(), 1);
    assert!(f.inbox().await["items"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn choosing_primary_is_space_scoped_versioned_and_does_not_grant_permissions() {
    let f = Fixture::new().await;
    let path = format!("/admin/v0/spaces/{}/primary-assistant", f.sid());
    let initial = call(&f.app, "GET", &path, ADMIN, json!({})).await;
    assert!(initial["primary_assistant"].is_null());
    assert_eq!(initial["revision"], 0);
    let chosen = call(
        &f.app,
        "PUT",
        &path,
        ADMIN,
        json!({"expected_revision":0,"actor_id":f.writer["actor_id"]}),
    )
    .await;
    assert_eq!(
        chosen["primary_assistant"]["actor_id"],
        f.writer["actor_id"]
    );
    assert_eq!(chosen["revision"], 1);
    assert_eq!(
        raw(
            &f.app,
            "PUT",
            &path,
            ADMIN,
            json!({"expected_revision":0,"actor_id":null})
        )
        .await
        .0,
        409
    );
    let user_connection = call(&f.app, "GET", "/v0/describe", &f.reader_token, json!({})).await;
    assert_eq!(
        user_connection["default_recipient_id"],
        f.space["default_actor_id"]
    );
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"other-space","display_name":"Other"}),
    )
    .await;
    assert_eq!(
        raw(
            &f.app,
            "PUT",
            &path,
            ADMIN,
            json!({"expected_revision":1,"actor_id":other["default_actor_id"]})
        )
        .await
        .0,
        404
    );
    call(
        &f.app,
        "PUT",
        &path,
        ADMIN,
        json!({"expected_revision":1,"actor_id":null}),
    )
    .await;
    assert!(call(&f.app, "GET", &path, ADMIN, json!({})).await["primary_assistant"].is_null());
}

#[tokio::test(flavor = "current_thread")]
async fn invite_redemption_is_one_time_and_establishes_only_its_personal_space() {
    let f = Fixture::new().await;
    let root = format!("/admin/v0/spaces/{}", f.sid());
    let invite = call(
        &f.app,
        "POST",
        &format!("{root}/invitations"),
        ADMIN,
        json!({"request_id":"invite-one"}),
    )
    .await;
    let raw_invite = invite["invite_token"].as_str().unwrap();
    let replay = call(
        &f.app,
        "POST",
        &format!("{root}/invitations"),
        ADMIN,
        json!({"request_id":"invite-one"}),
    )
    .await;
    assert!(replay["invite_token"].is_null());
    let response = f
        .app
        .handle(waypost::Request::json(
            "POST",
            "/admin/invitation",
            "",
            json!({"invite_token":raw_invite}),
        ))
        .await;
    assert_eq!(response.status, 200);
    let data: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let credential = data["data"]["credential"].as_str().unwrap();
    assert_eq!(data["data"]["space_id"], f.space["space_id"]);
    assert_eq!(data["data"]["operator"], false);
    assert!(response.headers["set-cookie"].contains("HttpOnly"));
    let own = call(
        &f.app,
        "POST",
        "/admin/v0/registry",
        credential,
        json!({"kind":"spaces"}),
    )
    .await;
    assert_eq!(own["items"].as_array().unwrap().len(), 1);
    assert_eq!(own["items"][0]["space_id"], f.space["space_id"]);
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/invitation",
            "",
            json!({"invite_token":raw_invite})
        )
        .await
        .0,
        401
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &format!("{root}/invitations"),
            credential,
            json!({"request_id":"user-cannot-invite"})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/v0/spaces",
            credential,
            json!({"request_id":"cannot-create-global","display_name":"Forbidden"})
        )
        .await
        .0,
        404
    );
}

#[tokio::test(flavor = "current_thread")]
async fn access_requests_are_private_and_idempotent_within_the_public_request_id() {
    let f = Fixture::new().await;
    let body = json!({"request_id":"access-123","email":"reader@example.com"});
    assert_eq!(
        raw(&f.app, "POST", "/admin/access-request", "", body.clone())
            .await
            .0,
        200
    );
    assert_eq!(
        raw(&f.app, "POST", "/admin/access-request", "", body)
            .await
            .0,
        200
    );
    let list = call(
        &f.app,
        "POST",
        "/admin/v0/access-requests",
        ADMIN,
        json!({"limit":20}),
    )
    .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["email"], "reader@example.com");
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/v0/access-requests",
            &f.reader_token,
            json!({"limit":20})
        )
        .await
        .0,
        401
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/access-request",
            "",
            json!({"request_id":"access-123","email":"another@example.com"})
        )
        .await
        .0,
        409
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/access-request",
            "",
            json!({"request_id":"invalid","email":"not-an-email"})
        )
        .await
        .0,
        400
    );
}

#[tokio::test(flavor = "current_thread")]
async fn revoked_expired_and_replaced_invitations_cannot_issue_personal_credentials() {
    use std::{cell::Cell, rc::Rc};
    struct Clock(Cell<i64>);
    impl waypost::Clock for Clock {
        fn now_ms(&self) -> i64 {
            self.0.get()
        }
    }
    let mut f = Fixture::new().await;
    let clock = Rc::new(Clock(Cell::new(1800000000000)));
    f.app = f.app.with_clock(clock.clone());
    let path = format!("/admin/v0/spaces/{}/invitations", f.sid());
    let first = call(
        &f.app,
        "POST",
        &path,
        ADMIN,
        json!({"request_id":"first","ttl_seconds":60}),
    )
    .await;
    let replacement = call(
        &f.app,
        "POST",
        &path,
        ADMIN,
        json!({"request_id":"replacement","ttl_seconds":60}),
    )
    .await;
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/invitation",
            "",
            json!({"invite_token":first["invite_token"]})
        )
        .await
        .0,
        401
    );
    call(
        &f.app,
        "DELETE",
        &format!("{path}/{}", replacement["invitation_id"].as_str().unwrap()),
        ADMIN,
        json!({}),
    )
    .await;
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/invitation",
            "",
            json!({"invite_token":replacement["invite_token"]})
        )
        .await
        .0,
        401
    );
    let expiring = call(
        &f.app,
        "POST",
        &path,
        ADMIN,
        json!({"request_id":"expiring","ttl_seconds":60}),
    )
    .await;
    clock.0.set(clock.0.get() + 60001);
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/invitation",
            "",
            json!({"invite_token":expiring["invite_token"]})
        )
        .await
        .0,
        401
    );
    let current = call(
        &f.app,
        "POST",
        &path,
        ADMIN,
        json!({"request_id":"current","ttl_seconds":60}),
    )
    .await;
    call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/owner-credential", f.sid()),
        ADMIN,
        json!({"expected_version":0}),
    )
    .await;
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/invitation",
            "",
            json!({"invite_token":current["invite_token"]})
        )
        .await
        .0,
        401
    );
}
