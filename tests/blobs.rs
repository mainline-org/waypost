mod common;
use common::*;
use proptest::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use waypost::Request;

fn upload_request(token: &str, id: &str, bytes: Vec<u8>) -> Request {
    let mut r = Request::json(
        "PUT",
        &format!("/v0/uploads/{id}/content"),
        token,
        json!({}),
    );
    r.body = bytes;
    r.headers
        .insert("content-type".into(), "application/octet-stream".into());
    r
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]
    #[test]
    fn immutable_uploads_verify_actual_bytes_and_survive_reopening(bytes in prop::collection::vec(any::<u8>(),0..1024)){
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let f=Fixture::new().await;
            let input=json!({"request_id":"prepare","content_type":"application/octet-stream","size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(&bytes))});
            let prepared=call(&f.app,"POST","/v0/uploads",&f.writer_token,input.clone()).await;
            assert_eq!(prepared,call(&f.app,"POST","/v0/uploads",&f.writer_token,input).await);
            let id=prepared["upload_id"].as_str().unwrap();
            let mut wrong=bytes.clone();if wrong.is_empty(){wrong.push(0)}else{wrong[0]^=1}
            let bad=f.app.handle(upload_request(&f.writer_token,id,wrong)).await;assert_eq!(bad.status,422);
            assert_eq!(f.app.handle(upload_request(&f.writer_token,id,bytes.clone())).await.status,200);
            let retry=f.app.handle(upload_request(&f.writer_token,id,bytes.clone())).await;
            let v:Value=serde_json::from_slice(&retry.body).unwrap();assert_eq!(v["data"]["already_uploaded"],true);
            let path=format!("/v0/uploads/{id}/commit");
            let sealed=call(&f.app,"POST",&path,&f.writer_token,json!({"request_id":"seal"})).await;
            assert_eq!(sealed,call(&f.app,"POST",&path,&f.writer_token,json!({"request_id":"seal"})).await);
            assert_eq!(f.app.handle(upload_request(&f.writer_token,id,bytes.clone())).await.status,409);
            let reopened=open(f.dir.path());let bid=sealed["blob_id"].as_str().unwrap();
            let r=reopened.handle(Request::json("GET",&format!("/v0/blobs/{bid}/content"),&f.writer_token,json!({}))).await;assert_eq!(r.status,200);assert_eq!(r.body,bytes);
            assert_eq!(raw(&reopened,"GET",&format!("/v0/blobs/{bid}"),&f.reader_token,json!({})).await.0,404);
            call(&reopened,"POST","/v0/messages",&f.writer_token,json!({"request_id":"with-attachment","kind":"info","subject":"Attached","body":{"text":"Portable bytes"},"attachment_ids":[bid]})).await;
            let visible=reopened.handle(Request::json("GET",&format!("/v0/blobs/{bid}/content"),&f.reader_token,json!({}))).await;assert_eq!(visible.status,200);assert_eq!(visible.body,bytes);
        });
    }
}
