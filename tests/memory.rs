mod common;
use common::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use waypost::Request;
async fn upload(f: &Fixture, key: &str, content: &[u8]) -> String {
    let digest = hex::encode(Sha256::digest(content));
    let prepared=call(&f.app,"POST","/v0/uploads",&f.writer_token,json!({"request_id":key,"content_type":"text/plain","size_bytes":content.len(),"sha256":digest})).await;
    let mut request = Request::json(
        "PUT",
        &format!(
            "/v0/uploads/{}/content",
            prepared["upload_id"].as_str().unwrap()
        ),
        &f.writer_token,
        json!({}),
    );
    request.body = content.to_vec();
    request
        .headers
        .insert("content-type".into(), "text/plain".into());
    let response = f.app.handle(request).await;
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{value}");
    let sealed = call(
        &f.app,
        "POST",
        &format!(
            "/v0/uploads/{}/commit",
            prepared["upload_id"].as_str().unwrap()
        ),
        &f.writer_token,
        json!({"request_id":format!("seal-{key}")}),
    )
    .await;
    sealed["blob_id"].as_str().unwrap().into()
}
#[tokio::test(flavor = "current_thread")]
async fn sealed_memory_snapshots_and_deltas_preserve_history_and_reject_stale_revisions() {
    let f = Fixture::new().await;
    let collection=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"memory-collection","source_actor_id":f.writer["actor_id"],"scope":"project/test","display_name":"Project"})).await;
    let cid = collection["collection_id"].as_str().unwrap();
    let blob = upload(&f, "note-upload", b"A portable decision").await;
    let path = format!("/v0/memory/collections/{cid}/revisions");
    let snapshot = json!({"request_id":"snapshot-1","collection_id":cid,"expected_revision":0,"mode":"snapshot","items":[{"item_id":"decision","blob_id":blob,"path":"decisions/first.md","metadata":{"origin":"user"}}],"delete_ids":[]});
    let first = call(&f.app, "POST", &path, &f.writer_token, snapshot.clone()).await;
    assert_eq!(first["revision"], 1);
    let repeated = call(&f.app, "POST", &path, &f.writer_token, snapshot).await;
    assert_eq!(first, repeated);
    let deleted=call(&f.app,"POST",&path,&f.writer_token,json!({"request_id":"delta-delete","expected_revision":1,"mode":"delta","items":[],"delete_ids":["decision"]})).await;
    assert_eq!(deleted["revision"], 2);
    let read = format!("/v0/memory/collections/{cid}/read");
    let old = call(
        &f.app,
        "POST",
        &read,
        &f.writer_token,
        json!({"revision":1,"limit":50}),
    )
    .await;
    assert_eq!(old["items"].as_array().unwrap().len(), 1);
    let current = call(
        &f.app,
        "POST",
        &read,
        &f.writer_token,
        json!({"include_tombstones":true,"limit":50}),
    )
    .await;
    assert_eq!(current["revision"], 2);
    assert_eq!(current["items"][0]["deleted_in_revision"], 2);
    let (status,error)=raw(&f.app,"POST",&path,&f.writer_token,json!({"request_id":"late-write","expected_revision":1,"mode":"delta","items":[{"item_id":"decision","blob_id":blob,"path":"decisions/first.md"}],"delete_ids":[]})).await;
    assert_eq!(status, 409);
    assert_eq!(error["error"]["code"], "REVISION_CONFLICT");
    let after = call(
        &f.app,
        "POST",
        &read,
        &f.writer_token,
        json!({"include_tombstones":true}),
    )
    .await;
    assert_eq!(after["revision"], 2);
}
#[tokio::test(flavor = "current_thread")]
async fn canonical_numeric_metadata_does_not_create_spurious_revisions() {
    let f = Fixture::new().await;
    let blob = upload(&f, "numeric-blob", b"Same note").await;
    let collection=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"numeric-collection","source_actor_id":f.writer["actor_id"],"scope":"numbers","display_name":"Numbers"})).await;
    let cid = collection["collection_id"].as_str().unwrap();
    let path = format!("/v0/memory/collections/{cid}/revisions");
    call(&f.app,"POST",&path,&f.writer_token,json!({"request_id":"numeric-first","expected_revision":0,"mode":"snapshot","items":[{"item_id":"note","blob_id":blob,"path":"note.md","metadata":{"count":1}}]})).await;
    let same=call(&f.app,"POST",&path,&f.writer_token,json!({"request_id":"numeric-same","expected_revision":1,"mode":"snapshot","items":[{"item_id":"note","blob_id":blob,"path":"note.md","metadata":{"count":1.0}}]})).await;
    assert_eq!(same["changed"], false);
    assert_eq!(same["revision"], 1);
    assert_eq!(f.writer_inbox().await["items"].as_array().unwrap().len(), 1);
}
