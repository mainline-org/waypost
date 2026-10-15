mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "current_thread")]
async fn owner_sees_actual_report_history_without_granting_assistants_admin_access() {
    let f = Fixture::new().await;
    let path = format!("/admin/v0/spaces/{}/connection-status", f.sid());
    let before = call(&f.app, "GET", &path, ADMIN, json!({})).await;
    assert_eq!(before["actors"][0]["report_count"], 0);
    assert!(before["actors"][0]["last_report_at"].is_null());
    f.send("first-report", "Finished a real task").await;
    f.send("first-report", "Finished a real task").await;
    let after = call(&f.app, "GET", &path, ADMIN, json!({})).await;
    assert_eq!(after["actors"][0]["actor_id"], f.writer["actor_id"]);
    assert_eq!(after["actors"][0]["report_count"], 1);
    assert!(after["actors"][0]["last_report_at"].is_string());
    assert_eq!(
        raw(&f.app, "GET", &path, &f.writer_token, json!({}))
            .await
            .0,
        401
    );
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"status-other","display_name":"Other"}),
    )
    .await;
    let owner = call(
        &f.app,
        "POST",
        &format!(
            "/admin/v0/spaces/{}/owner-credential",
            other["space_id"].as_str().unwrap()
        ),
        ADMIN,
        json!({"expected_version":0}),
    )
    .await;
    assert_eq!(
        raw(
            &f.app,
            "GET",
            &path,
            owner["credential"].as_str().unwrap(),
            json!({})
        )
        .await
        .0,
        404
    );
}
