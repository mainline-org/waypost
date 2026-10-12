mod common;
use common::*;
use serde_json::json;
use waypost::Request;
#[tokio::test(flavor = "current_thread")]
async fn delegated_write_does_not_turn_a_private_full_manifest_into_the_writers_owned_blob() {
    let f = Fixture::new().await;
    let collection=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"private-source","source_actor_id":f.writer["actor_id"],"scope":"private-source","display_name":"Private"})).await;
    let cid = collection["collection_id"].as_str().unwrap();
    let prepare = |key: &str| json!({"request_id":key,"content_type":"text/plain","size_bytes":0,"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"});
    let p = call(
        &f.app,
        "POST",
        "/v0/uploads",
        &f.writer_token,
        prepare("original"),
    )
    .await;
    let mut r = Request::json(
        "PUT",
        &format!("/v0/uploads/{}/content", p["upload_id"].as_str().unwrap()),
        &f.writer_token,
        json!({}),
    );
    r.body = vec![];
    r.headers.insert("content-type".into(), "text/plain".into());
    assert_eq!(f.app.handle(r).await.status, 200);
    let blob = call(
        &f.app,
        "POST",
        &format!("/v0/uploads/{}/commit", p["upload_id"].as_str().unwrap()),
        &f.writer_token,
        json!({"request_id":"seal-original"}),
    )
    .await;
    let path = format!("/v0/memory/collections/{cid}/revisions");
    call(&f.app,"POST",&path,&f.writer_token,json!({"request_id":"seed","expected_revision":0,"mode":"snapshot","items":[{"item_id":"private","blob_id":blob["blob_id"],"path":"private.md","metadata":{"secret_note":"Source-private metadata"}}]})).await;
    let actor = call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/actors", f.sid()),
        ADMIN,
        json!({"request_id":"delegate","display_name":"Write only","kind":"agent"}),
    )
    .await;
    let consumer=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/consumers",f.sid()),ADMIN,json!({"request_id":"delegate-consumer","actor_id":actor["actor_id"],"view":"inbox","filter":{}})).await;
    let token=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",f.sid()),ADMIN,json!({"request_id":"delegate-token","actor_id":actor["actor_id"],"consumer_id":consumer["consumer_id"],"scopes":["memory.write","blob.write"],"resource_grants":{"memory_write_collection_ids":[cid]}})).await;
    let token = token["token"].as_str().unwrap();
    let p = call(&f.app, "POST", "/v0/uploads", token, prepare("addition")).await;
    let mut r = Request::json(
        "PUT",
        &format!("/v0/uploads/{}/content", p["upload_id"].as_str().unwrap()),
        token,
        json!({}),
    );
    r.body = vec![];
    r.headers.insert("content-type".into(), "text/plain".into());
    assert_eq!(f.app.handle(r).await.status, 200);
    let blob = call(
        &f.app,
        "POST",
        &format!("/v0/uploads/{}/commit", p["upload_id"].as_str().unwrap()),
        token,
        json!({"request_id":"seal-addition"}),
    )
    .await;
    let result=call(&f.app,"POST",&path,token,json!({"request_id":"delegated-delta","expected_revision":1,"mode":"delta","items":[{"item_id":"added","blob_id":blob["blob_id"],"path":"added.md"}]})).await;
    assert_eq!(result["revision"], 2);
    assert_eq!(
        raw(
            &f.app,
            "GET",
            &format!("/v0/blobs/{}", result["manifest_blob_id"].as_str().unwrap()),
            token,
            json!({})
        )
        .await
        .0,
        404
    );
    let data = call(
        &f.app,
        "POST",
        &format!("/v0/memory/collections/{cid}/read"),
        &f.writer_token,
        json!({}),
    )
    .await;
    assert_eq!(data["items"].as_array().unwrap().len(), 2);
}
