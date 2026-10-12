#![allow(dead_code)]
use serde_json::{Value, json};
use waypost::native::{FileStore, SqliteDb};
use waypost::{App, Config, Request};
pub const ADMIN: &str = "test-only-admin-secret-32-bytes-long";
pub type TestApp = App<SqliteDb, FileStore>;
pub async fn raw(
    app: &TestApp,
    method: &str,
    path: &str,
    token: &str,
    body: Value,
) -> (u16, Value) {
    let r = app.handle(Request::json(method, path, token, body)).await;
    let v = serde_json::from_slice(&r.body).unwrap();
    (r.status, v)
}
pub async fn call(app: &TestApp, method: &str, path: &str, token: &str, body: Value) -> Value {
    let (status, v) = raw(app, method, path, token, body).await;
    assert_eq!(status, 200, "{path}: {v}");
    assert_eq!(v["ok"], true, "{path}: {v}");
    v["data"].clone()
}
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub app: TestApp,
    pub space: Value,
    pub writer: Value,
    pub writer_consumer: Value,
    pub writer_token: String,
    pub reader_token: String,
}
impl Fixture {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let app = open(dir.path());
        let space = call(
            &app,
            "POST",
            "/admin/v0/spaces",
            ADMIN,
            json!({"request_id":"setup-space","display_name":"Test"}),
        )
        .await;
        let sid = space["space_id"].as_str().unwrap();
        let writer = call(
            &app,
            "POST",
            &format!("/admin/v0/spaces/{sid}/actors"),
            ADMIN,
            json!({"request_id":"setup-writer","display_name":"Writer","kind":"agent"}),
        )
        .await;
        let writer_consumer=call(&app,"POST",&format!("/admin/v0/spaces/{sid}/consumers"),ADMIN,json!({"request_id":"setup-writer-consumer","actor_id":writer["actor_id"],"view":"inbox","filter":{}})).await;
        let token=call(&app,"POST",&format!("/admin/v0/spaces/{sid}/tokens"),ADMIN,json!({"request_id":"setup-writer-token","actor_id":writer["actor_id"],"consumer_id":writer_consumer["consumer_id"],"scopes":["message.send","message.read","receipt.write","blob.write","memory.read","memory.write"],"resource_grants":{}})).await;
        let reader=call(&app,"POST",&format!("/admin/v0/spaces/{sid}/tokens"),ADMIN,json!({"request_id":"setup-reader-token","actor_id":space["default_actor_id"],"consumer_id":space["default_consumer_id"],"scopes":["message.send","message.read","receipt.write","memory.read"],"resource_grants":{"recipient_ids":[writer["actor_id"]]}})).await;
        Self {
            dir,
            app,
            space,
            writer,
            writer_consumer,
            writer_token: token["token"].as_str().unwrap().into(),
            reader_token: reader["token"].as_str().unwrap().into(),
        }
    }
    pub fn sid(&self) -> &str {
        self.space["space_id"].as_str().unwrap()
    }
    pub async fn send(&self, key: &str, text: &str) -> Value {
        call(&self.app,"POST","/v0/messages",&self.writer_token,json!({"request_id":key,"kind":"info","subject":"Test message","body":{"text":text},"metadata":{}})).await
    }
    pub async fn inbox(&self) -> Value {
        call(
            &self.app,
            "POST",
            "/v0/inbox/read",
            &self.reader_token,
            json!({"consumer_id":self.space["default_consumer_id"],"limit":50,"cursor":null}),
        )
        .await
    }
    pub async fn writer_inbox(&self) -> Value {
        call(
            &self.app,
            "POST",
            "/v0/inbox/read",
            &self.writer_token,
            json!({"consumer_id":self.writer_consumer["consumer_id"],"limit":50,"cursor":null}),
        )
        .await
    }
}
pub fn open(path: &std::path::Path) -> TestApp {
    App::new(
        SqliteDb::open(path.join("metadata.sqlite")).unwrap(),
        FileStore::open(path.join("objects")).unwrap(),
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    )
}
