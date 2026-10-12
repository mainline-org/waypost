mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "current_thread")]
async fn explicit_collection_grants_deliver_version_events_and_fixed_revision_pages() {
    let f = Fixture::new().await;
    let c=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"feed-collection","source_actor_id":f.writer["actor_id"],"scope":"portable","display_name":"Portable"})).await;
    let cid = c["collection_id"].as_str().unwrap();
    let writer_token = f.writer_token.clone();
    let mut input = waypost::Request::json("PUT", "", &writer_token, json!({}));
    let prepared=call(&f.app,"POST","/v0/uploads",&writer_token,json!({"request_id":"empty-blob","content_type":"text/plain","size_bytes":0,"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"})).await;
    input.path = format!(
        "/v0/uploads/{}/content",
        prepared["upload_id"].as_str().unwrap()
    );
    input.body = vec![];
    input
        .headers
        .insert("content-type".into(), "text/plain".into());
    assert_eq!(f.app.handle(input).await.status, 200);
    let sealed = call(
        &f.app,
        "POST",
        &format!(
            "/v0/uploads/{}/commit",
            prepared["upload_id"].as_str().unwrap()
        ),
        &writer_token,
        json!({"request_id":"seal-empty"}),
    )
    .await;
    let publish = format!("/v0/memory/collections/{cid}/revisions");
    let read = format!("/v0/memory/collections/{cid}/read");
    let v=call(&f.app,"POST",&publish,&writer_token,json!({"request_id":"first","expected_revision":0,"mode":"snapshot","items":[{"item_id":"a","blob_id":sealed["blob_id"],"path":"a.md"},{"item_id":"b","blob_id":sealed["blob_id"],"path":"b.md"}]})).await;
    assert_eq!(v["revision"], 1);
    assert_eq!(
        raw(&f.app, "POST", &read, &f.reader_token, json!({}))
            .await
            .0,
        404
    );
    let reader=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",f.sid()),ADMIN,json!({"request_id":"memory-reader","actor_id":f.space["default_actor_id"],"consumer_id":f.space["default_consumer_id"],"scopes":["memory.read","receipt.write"],"resource_grants":{"memory_read_collection_ids":[cid]}})).await;
    let token = reader["token"].as_str().unwrap();
    let feed = call(
        &f.app,
        "POST",
        "/v0/inbox/read",
        token,
        json!({"consumer_id":f.space["default_consumer_id"]}),
    )
    .await;
    assert_eq!(feed["items"].as_array().unwrap().len(), 1);
    assert_eq!(feed["items"][0]["item_type"], "memory_revision");
    assert_eq!(feed["items"][0]["data"]["revision"], 1);
    let page = call(&f.app, "POST", &read, token, json!({"limit":1})).await;
    assert_eq!(page["items"][0]["item_id"], "a");
    assert_eq!(page["has_more"], true);
    call(
        &f.app,
        "POST",
        &publish,
        &writer_token,
        json!({"request_id":"clear","expected_revision":1,"mode":"snapshot","items":[]}),
    )
    .await;
    let old_page = call(
        &f.app,
        "POST",
        &read,
        token,
        json!({"limit":1,"cursor":page["next_cursor"]}),
    )
    .await;
    assert_eq!(old_page["revision"], 1);
    assert_eq!(old_page["items"][0]["item_id"], "b");
    let unchanged = call(
        &f.app,
        "POST",
        &publish,
        &writer_token,
        json!({"request_id":"clear-again","expected_revision":2,"mode":"snapshot","items":[]}),
    )
    .await;
    assert_eq!(unchanged["changed"], false);
    assert_eq!(unchanged["revision"], 2);
    let feed2 = call(
        &f.app,
        "POST",
        "/v0/inbox/read",
        token,
        json!({"consumer_id":f.space["default_consumer_id"]}),
    )
    .await;
    assert_eq!(feed2["items"].as_array().unwrap().len(), 2);
    let blob = call(
        &f.app,
        "GET",
        &format!("/v0/blobs/{}", sealed["blob_id"].as_str().unwrap()),
        token,
        json!({}),
    )
    .await;
    assert_eq!(blob["size_bytes"], 0);
}
