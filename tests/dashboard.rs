mod common;
use common::*;
use serde_json::{Value, json};
use waypost::Request;
#[tokio::test(flavor = "current_thread")]
async fn admin_sessions_expire_on_logout_and_inspection_does_not_acknowledge_messages() {
    let f = Fixture::new().await;
    f.send("inspect", "<script>unsafe</script>").await;
    let response = f
        .app
        .handle(Request::json(
            "POST",
            "/admin/session",
            "",
            json!({"admin_secret":ADMIN}),
        ))
        .await;
    assert_eq!(response.status, 200);
    let data: Value = serde_json::from_slice(&response.body).unwrap();
    let csrf = data["data"]["csrf_token"].as_str().unwrap();
    let cookie = response.headers["set-cookie"].split(';').next().unwrap();
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let secret = cookie.split_once('=').unwrap().1.as_bytes();
    for part in csrf.split('.') {
        if let Ok(decoded) = URL_SAFE_NO_PAD.decode(part) {
            assert!(
                !decoded.windows(secret.len()).any(|bytes| bytes == secret),
                "CSRF token must not disclose the HttpOnly session credential"
            );
        }
    }

    let mut request = Request::json(
        "POST",
        &format!("/admin/v0/spaces/{}/inspect", f.sid()),
        "",
        json!({"kind":"messages","limit":1}),
    );
    request.headers.remove("authorization");
    request.headers.insert("cookie".into(), cookie.into());
    request.headers.insert("x-waypost-csrf".into(), csrf.into());
    let r = f.app.handle(request).await;
    assert_eq!(r.status, 200);
    let data: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(data["data"]["items"].as_array().unwrap().len(), 1);
    assert!(
        f.writer_inbox().await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut logout = Request::json("DELETE", "/admin/session", "", json!({}));
    logout.headers.remove("authorization");
    logout.headers.insert("cookie".into(), cookie.into());
    logout.headers.insert("x-waypost-csrf".into(), csrf.into());
    assert_eq!(f.app.handle(logout).await.status, 200);
    let mut request = Request::json("GET", "/admin/v0/spaces", "", json!({}));
    request.headers.remove("authorization");
    request.headers.insert("cookie".into(), cookie.into());
    assert_eq!(f.app.handle(request).await.status, 401);
    let page = f
        .app
        .handle(Request::json("GET", "/dashboard", "", json!({})))
        .await;
    assert_eq!(page.status, 200);
    assert!(page.headers["content-type"].contains("text/html"));
    assert!(page.headers.contains_key("content-security-policy"));
}
