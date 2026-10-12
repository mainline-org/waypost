use serde_json::{Value, json};
use waypost::native::{FileStore, SqliteDb};
use waypost::{App, Config, Request};

const ADMIN: &str = "test-only-admin-secret-32-bytes-long";

async fn call(
    app: &App<SqliteDb, FileStore>,
    method: &str,
    path: &str,
    token: &str,
    body: Value,
) -> Value {
    let response = app.handle(Request::json(method, path, token, body)).await;
    let result: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{path}: {result}");
    assert_eq!(result["ok"], true, "{path}: {result}");
    result["data"].clone()
}

#[tokio::test(flavor = "current_thread")]
async fn a_producer_can_publish_once_and_an_independent_consumer_reads_the_exact_content() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(
        SqliteDb::open(dir.path().join("metadata.sqlite")).unwrap(),
        FileStore::open(dir.path().join("objects")).unwrap(),
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    );
    let space = call(
        &app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"setup-space","display_name":"Mail test"}),
    )
    .await;
    let sid = space["space_id"].as_str().unwrap();
    let actor = call(
        &app,
        "POST",
        &format!("/admin/v0/spaces/{sid}/actors"),
        ADMIN,
        json!({"request_id":"setup-writer","display_name":"Writer","kind":"agent"}),
    )
    .await;
    let actor_id = actor["actor_id"].as_str().unwrap();
    let consumer = call(&app, "POST", &format!("/admin/v0/spaces/{sid}/consumers"), ADMIN,
        json!({"request_id":"setup-writer-consumer","actor_id":actor_id,"view":"inbox","filter":{}})).await;
    let sender = call(&app, "POST", &format!("/admin/v0/spaces/{sid}/tokens"), ADMIN,
        json!({"request_id":"setup-writer-token","actor_id":actor_id,"consumer_id":consumer["consumer_id"],
            "scopes":["message.send","message.read"],"resource_grants":{}})).await;
    let sender_token = sender["token"].as_str().unwrap();
    let reader = call(&app, "POST", &format!("/admin/v0/spaces/{sid}/tokens"), ADMIN,
        json!({"request_id":"setup-reader-token","actor_id":space["default_actor_id"],"consumer_id":space["default_consumer_id"],
            "scopes":["message.read","receipt.write"],"resource_grants":{}})).await;
    let reader_token = reader["token"].as_str().unwrap();
    let message = json!({"request_id":"report-001","kind":"result","subject":"A durable report",
        "body":{"text":"Exact original content"},"attachment_ids":[],"metadata":{}});
    let published = call(&app, "POST", "/v0/messages", sender_token, message.clone()).await;
    let repeated = call(&app, "POST", "/v0/messages", sender_token, message).await;
    assert_eq!(published["resource_id"], repeated["resource_id"]);
    assert_eq!(published["committed_at"], repeated["committed_at"]);
    let page = call(
        &app,
        "POST",
        "/v0/inbox/read",
        reader_token,
        json!({"consumer_id":space["default_consumer_id"],"cursor":null,"limit":1}),
    )
    .await;
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["resource_id"], published["resource_id"]);
    let original = call(
        &app,
        "GET",
        &format!(
            "/v0/messages/{}",
            published["resource_id"].as_str().unwrap()
        ),
        reader_token,
        json!({}),
    )
    .await;
    assert_eq!(original["body"]["text"], "Exact original content");
}
