mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "current_thread")]
async fn committed_reply_recovers_by_actor_with_a_send_only_rotated_credential() {
    let f = Fixture::new().await;
    let parent=call(&f.app,"POST","/v0/messages",&f.reader_token,json!({"request_id":"parent","recipient_ids":[f.writer["actor_id"]],"kind":"question","subject":"Question","body":{"text":"Reply here"}})).await;
    let input = json!({"request_id":"reply","recipient_ids":[f.space["default_actor_id"]],"in_reply_to":parent["message_id"],"kind":"result","subject":"Answer","body":{"text":"Saved answer"}});
    let original = call(
        &f.app,
        "POST",
        "/v0/messages",
        &f.writer_token,
        input.clone(),
    )
    .await;
    let token=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",f.sid()),ADMIN,json!({"request_id":"send-only-rotation","actor_id":f.writer["actor_id"],"consumer_id":f.writer_consumer["consumer_id"],"scopes":["message.send"],"resource_grants":{}})).await;
    let token = token["token"].as_str().unwrap();
    assert_eq!(
        original,
        call(&f.app, "POST", "/v0/messages", token, input.clone()).await
    );
    let mut new = input;
    new["request_id"] = json!("new-reply-without-read");
    assert_eq!(raw(&f.app, "POST", "/v0/messages", token, new).await.0, 403);
    assert_eq!(f.inbox().await["items"].as_array().unwrap().len(), 1);
}
