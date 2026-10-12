mod common;
use common::*;
use serde_json::json;
#[tokio::test(flavor = "current_thread")]
async fn privacy_block_is_a_stable_failed_submission_after_reservation() {
    let f = Fixture::new().await;
    let policy = format!("/admin/v0/spaces/{}/privacy", f.sid());
    call(&f.app,"PUT",&policy,ADMIN,json!({"expected_revision":0,"enabled":true,"mode":"block","all_agents":true,"actor_ids":[],"rules":[{"kind":"email"}]})).await;
    let input = json!({"request_id":"blocked-ledger","kind":"info","subject":"Contact","body":{"text":"private@example.org"}});
    let (status, error) = raw(
        &f.app,
        "POST",
        "/v0/messages",
        &f.writer_token,
        input.clone(),
    )
    .await;
    assert_eq!(status, 422);
    let state = call(
        &f.app,
        "POST",
        "/v0/submissions/status",
        &f.writer_token,
        json!({"operation":"message.send","request_id":"blocked-ledger"}),
    )
    .await;
    assert_eq!(state["submission_state"], "failed");
    assert_eq!(state["error"]["code"], "PRIVACY_BLOCKED");
    call(&f.app,"PUT",&policy,ADMIN,json!({"expected_revision":1,"enabled":false,"mode":"redact","all_agents":true,"actor_ids":[],"rules":[]})).await;
    let repeated = raw(&f.app, "POST", "/v0/messages", &f.writer_token, input).await;
    assert_eq!(repeated.0, status);
    assert_eq!(repeated.1["error"], error["error"]);
    assert!(f.inbox().await["items"].as_array().unwrap().is_empty());
}
