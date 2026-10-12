mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "current_thread")]
async fn selected_agent_messages_are_filtered_without_exposing_originals_or_changing_retry_results()
{
    let f = Fixture::new().await;
    let policy = format!("/admin/v0/spaces/{}/privacy", f.sid());
    let cfg = json!({"expected_revision":0,"enabled":true,"mode":"redact","all_agents":false,"actor_ids":[f.writer["actor_id"]],"allow_uninspected_attachments":false,"rules":[{"kind":"email"},{"kind":"credential"},{"kind":"metadata_key","value":"password"},{"kind":"literal","value":"Project Secret"}]});
    call(&f.app, "PUT", &policy, ADMIN, cfg).await;
    let input = json!({"request_id":"private-message","kind":"info","subject":"Project Secret","body":{"text":"Contact alice@example.org. Project Secret. sk-test01234567890123456789"},"metadata":{"password":"hunter2","alice@example.org":"private"}});
    let sent = call(
        &f.app,
        "POST",
        "/v0/messages",
        &f.writer_token,
        input.clone(),
    )
    .await;
    let id = sent["message_id"].as_str().unwrap();
    let message = call(
        &f.app,
        "GET",
        &format!("/v0/messages/{id}"),
        &f.reader_token,
        json!({}),
    )
    .await;
    let text = message.to_string();
    for private in [
        "Project Secret",
        "alice@example.org",
        "hunter2",
        "sk-test01234567890123456789",
    ] {
        assert!(!text.contains(private), "{text}")
    }
    assert!(text.contains("[REDACTED]"));
    assert!(message.get("request_id").is_none());
    let audit = call(&f.app, "GET", &format!("{policy}/audit"), ADMIN, json!({})).await;
    assert!(!audit.to_string().contains("alice@example.org"));
    assert!(!audit.to_string().contains("hunter2"));
    call(&f.app,"PUT",&policy,ADMIN,json!({"expected_revision":1,"enabled":true,"mode":"block","all_agents":true,"actor_ids":[],"allow_uninspected_attachments":false,"rules":[{"kind":"email"}]})).await;
    assert_eq!(
        sent,
        call(&f.app, "POST", "/v0/messages", &f.writer_token, input).await
    );
    let (status,error)=raw(&f.app,"POST","/v0/messages",&f.writer_token,json!({"request_id":"blocked","kind":"info","subject":"Private","body":{"text":"bob@example.org"}})).await;
    assert_eq!(status, 422);
    assert_eq!(error["error"]["code"], "PRIVACY_BLOCKED");
    assert_eq!(f.inbox().await["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        raw(&f.app, "PUT", &policy, &f.writer_token, json!({}))
            .await
            .0,
        401
    );
}
#[tokio::test(flavor = "current_thread")]
async fn privacy_filters_recipient_traffic_and_text_attachments_but_blocks_uninspectable_bytes() {
    use sha2::{Digest, Sha256};
    use waypost::Request;
    let f = Fixture::new().await;
    call(&f.app,"PUT",&format!("/admin/v0/spaces/{}/privacy",f.sid()),ADMIN,json!({"expected_revision":0,"enabled":true,"mode":"redact","all_agents":false,"actor_ids":[f.writer["actor_id"]],"rules":[{"kind":"email"}]})).await;
    let from_owner=call(&f.app,"POST","/v0/messages",&f.reader_token,json!({"request_id":"recipient-filter","recipient_ids":[f.writer["actor_id"]],"kind":"info","subject":"Contact","body":{"text":"owner@example.org"}})).await;
    let m = call(
        &f.app,
        "GET",
        &format!(
            "/v0/messages/{}",
            from_owner["message_id"].as_str().unwrap()
        ),
        &f.writer_token,
        json!({}),
    )
    .await;
    assert_eq!(m["body"]["text"], "[REDACTED]");
    for (kind, content, key) in [
        ("text/plain", b"agent@example.org".as_slice(), "text"),
        ("application/pdf", b"uninspectable".as_slice(), "binary"),
    ] {
        let prep=call(&f.app,"POST","/v0/uploads",&f.writer_token,json!({"request_id":format!("prepare-{key}"),"content_type":kind,"size_bytes":content.len(),"sha256":hex::encode(Sha256::digest(content)),"filename":"agent@example.org.txt"})).await;
        let mut req = Request::json(
            "PUT",
            &format!(
                "/v0/uploads/{}/content",
                prep["upload_id"].as_str().unwrap()
            ),
            &f.writer_token,
            json!({}),
        );
        req.headers.insert("content-type".into(), kind.into());
        req.body = content.to_vec();
        assert_eq!(f.app.handle(req).await.status, 200);
        let blob = call(
            &f.app,
            "POST",
            &format!("/v0/uploads/{}/commit", prep["upload_id"].as_str().unwrap()),
            &f.writer_token,
            json!({"request_id":format!("seal-{key}")}),
        )
        .await;
        let (status,sent)=raw(&f.app,"POST","/v0/messages",&f.writer_token,json!({"request_id":format!("send-{key}"),"kind":"info","subject":"Attachment","body":{"text":"See attachment"},"attachment_ids":[blob["blob_id"]]})).await;
        if key == "binary" {
            assert_eq!(status, 422);
            continue;
        }
        assert_eq!(status, 200);
        let m = call(
            &f.app,
            "GET",
            &format!(
                "/v0/messages/{}",
                sent["data"]["message_id"].as_str().unwrap()
            ),
            &f.reader_token,
            json!({}),
        )
        .await;
        assert_ne!(m["attachment_ids"][0], blob["blob_id"]);
        let id = m["attachment_ids"][0].as_str().unwrap();
        let r = f
            .app
            .handle(Request::json(
                "GET",
                &format!("/v0/blobs/{id}/content"),
                &f.reader_token,
                json!({}),
            ))
            .await;
        assert_eq!(r.status, 200);
        assert_eq!(r.body, b"[REDACTED]");
        let metadata = call(
            &f.app,
            "GET",
            &format!("/v0/blobs/{id}"),
            &f.reader_token,
            json!({}),
        )
        .await;
        assert_eq!(metadata["filename"], "[REDACTED]");
        assert_eq!(
            raw(
                &f.app,
                "GET",
                &format!("/v0/blobs/{}", blob["blob_id"].as_str().unwrap()),
                &f.reader_token,
                json!({})
            )
            .await
            .0,
            404
        );
    }
    assert_eq!(f.inbox().await["items"].as_array().unwrap().len(), 1);
}
#[tokio::test(flavor = "current_thread")]
async fn expanding_privacy_rules_reject_content_at_the_output_budget() {
    let f = Fixture::new().await;
    call(&f.app,"PUT",&format!("/admin/v0/spaces/{}/privacy",f.sid()),ADMIN,json!({"expected_revision":0,"enabled":true,"mode":"redact","all_agents":true,"actor_ids":[],"rules":[{"kind":"regex","value":"a*"},{"kind":"regex","value":"b*"},{"kind":"regex","value":"c*"}]})).await;
    let (status,error)=raw(&f.app,"POST","/v0/messages",&f.writer_token,json!({"request_id":"expansion","kind":"info","subject":"Budget","body":{"text":"x".repeat(2000)}})).await;
    assert_eq!(status, 413);
    assert_eq!(error["error"]["code"], "RESOURCE_LIMIT");
    assert!(f.inbox().await["items"].as_array().unwrap().is_empty());
}
