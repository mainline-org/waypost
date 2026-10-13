mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "current_thread")]
async fn dashboard_inbox_shows_only_owner_deliveries_and_binds_its_cursor() {
    let f = Fixture::new().await;
    let incoming = f.send("incoming", "A report for the owner").await;
    let second = f.send("incoming-2", "Another report").await;
    call(&f.app, "POST", "/v0/messages", &f.reader_token,
        json!({"request_id":"to-agent","recipient_ids":[f.writer["actor_id"]],"kind":"info","subject":"For the agent","body":{"text":"Different destination"},"metadata":{}})).await;
    let path = format!("/admin/v0/spaces/{}/inspect", f.sid());
    let inbox = call(
        &f.app,
        "POST",
        &path,
        ADMIN,
        json!({"kind":"inbox","limit":50}),
    )
    .await;
    let ids: Vec<_> = inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["message_id"].clone())
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&incoming["message_id"]));
    assert!(ids.contains(&second["message_id"]));
    let all = call(
        &f.app,
        "POST",
        &path,
        ADMIN,
        json!({"kind":"messages","limit":50}),
    )
    .await;
    assert_eq!(all["items"].as_array().unwrap().len(), 3);
    let first = call(
        &f.app,
        "POST",
        &path,
        ADMIN,
        json!({"kind":"inbox","limit":1}),
    )
    .await;
    assert!(first["has_more"].as_bool().unwrap());
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &path,
            ADMIN,
            json!({"kind":"messages","cursor":first["next_cursor"],"limit":1})
        )
        .await
        .0,
        400
    );
    assert_eq!(raw(&f.app, "POST", &path, ADMIN,
        json!({"kind":"inbox","actor_id":f.writer["actor_id"],"cursor":first["next_cursor"],"limit":1})).await.0, 400);
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"other","display_name":"Other user"}),
    )
    .await;
    let other_path = format!(
        "/admin/v0/spaces/{}/inspect",
        other["space_id"].as_str().unwrap()
    );
    assert!(
        call(&f.app, "POST", &other_path, ADMIN, json!({"kind":"inbox"})).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Inspection never sends an Agent receipt.
    assert!(
        f.writer_inbox().await["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["receipt"].is_null())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn sender_connection_can_report_and_recover_without_inbox_or_data_access() {
    let f = Fixture::new().await;
    let path = format!("/admin/v0/spaces/{}/tokens", f.sid());
    let credential = call(&f.app,"POST",&path,ADMIN,json!({"request_id":"sender-only","actor_id":f.writer["actor_id"],"consumer_id":f.writer_consumer["consumer_id"],"scopes":["message.send"],"resource_grants":{}})).await;
    let token = credential["token"].as_str().unwrap();
    let describe = call(&f.app, "GET", "/v0/describe", token, json!({})).await;
    assert_eq!(describe["scopes"], json!(["message.send"]));
    assert_eq!(
        describe["allowed_recipient_ids"],
        json!([f.space["default_actor_id"]])
    );
    let sent = call(&f.app,"POST","/v0/messages",token,json!({"request_id":"first-report","kind":"info","subject":"Connected","body":{"text":"My first report"},"metadata":{}})).await;
    let status = call(
        &f.app,
        "POST",
        "/v0/submissions/status",
        token,
        json!({"request_id":"first-report","operation":"message.send"}),
    )
    .await;
    assert_eq!(status["submission_state"], "committed");
    assert_eq!(status["resource_id"], sent["message_id"]);
    for (route, body) in [
        (
            "/v0/inbox/read",
            json!({"consumer_id":f.writer_consumer["consumer_id"]}),
        ),
        ("/v0/uploads", json!({})),
        ("/v0/receipts", json!({})),
    ] {
        assert_eq!(
            raw(&f.app, "POST", route, token, body).await.0,
            403,
            "{route}"
        );
    }
    assert_eq!(raw(&f.app,"POST","/v0/messages",token,json!({"request_id":"unauthorized-recipient","recipient_ids":[f.writer["actor_id"]],"kind":"info","subject":"Forbidden","body":{"text":"No"},"metadata":{}})).await.0,403);
    let inspect = call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/inspect", f.sid()),
        ADMIN,
        json!({"kind":"tokens","actor_id":f.writer["actor_id"]}),
    )
    .await;
    let stored = inspect["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["token_id"] == credential["token_id"])
        .unwrap();
    assert_eq!(stored["scopes"], json!(["message.send"]));
    assert!(stored.get("token").is_none());
    assert!(stored.get("token_digest").is_none());
    call(
        &f.app,
        "DELETE",
        &format!("{path}/{}", credential["token_id"].as_str().unwrap()),
        ADMIN,
        json!({}),
    )
    .await;
    assert_eq!(
        raw(&f.app, "GET", "/v0/describe", token, json!({})).await.0,
        401
    );
}
