mod common;
use common::*;
use serde_json::json;
use waypost::Request;
#[tokio::test(flavor = "current_thread")]
async fn space_observation_never_grants_unshared_blobs_or_ungranted_memory() {
    let f = Fixture::new().await;
    let c=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"private-collection","source_actor_id":f.writer["actor_id"],"scope":"private","display_name":"Private"})).await;
    let prep=call(&f.app,"POST","/v0/uploads",&f.writer_token,json!({"request_id":"private-bytes","size_bytes":0,"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","content_type":"text/plain"})).await;
    let mut req = Request::json(
        "PUT",
        &format!(
            "/v0/uploads/{}/content",
            prep["upload_id"].as_str().unwrap()
        ),
        &f.writer_token,
        json!({}),
    );
    req.body = vec![];
    req.headers
        .insert("content-type".into(), "text/plain".into());
    assert_eq!(f.app.handle(req).await.status, 200);
    let blob = call(
        &f.app,
        "POST",
        &format!("/v0/uploads/{}/commit", prep["upload_id"].as_str().unwrap()),
        &f.writer_token,
        json!({"request_id":"seal-private"}),
    )
    .await;
    let cid = c["collection_id"].as_str().unwrap();
    call(&f.app,"POST",&format!("/v0/memory/collections/{cid}/revisions"),&f.writer_token,json!({"request_id":"private-revision","expected_revision":0,"mode":"snapshot","items":[{"item_id":"private","blob_id":blob["blob_id"],"path":"private.md"}]})).await;
    let consumer=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/consumers",f.sid()),ADMIN,json!({"request_id":"observer-consumer","actor_id":f.space["default_actor_id"],"view":"space_log","filter":{}})).await;
    let token=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",f.sid()),ADMIN,json!({"request_id":"observer-token","actor_id":f.space["default_actor_id"],"consumer_id":consumer["consumer_id"],"scopes":["space.observe","memory.read"],"resource_grants":{}})).await;
    let token = token["token"].as_str().unwrap();
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &format!("/v0/memory/collections/{cid}/read"),
            token,
            json!({})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "GET",
            &format!("/v0/blobs/{}", blob["blob_id"].as_str().unwrap()),
            token,
            json!({})
        )
        .await
        .0,
        404
    );
    let feed = call(
        &f.app,
        "POST",
        "/v0/inbox/read",
        token,
        json!({"consumer_id":consumer["consumer_id"]}),
    )
    .await;
    assert!(feed["items"].as_array().unwrap().is_empty());
    let message = f.send("observable-message", "Public to observer").await;
    assert_eq!(
        call(
            &f.app,
            "GET",
            &format!("/v0/messages/{}", message["message_id"].as_str().unwrap()),
            token,
            json!({})
        )
        .await["body"]["text"],
        "Public to observer"
    );
}
