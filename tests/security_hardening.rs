mod common;
use common::*;
use futures_util::future::LocalBoxFuture;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use waypost::{
    App, Clock, Config, Request,
    error::{Error, Result},
    native::{FileStore, SqliteDb},
    storage::{ObjectInfo, ObjectStore},
};
const CLEANUP: &str = "/admin/v0/maintenance/cleanup";
struct TestClock(Cell<i64>);
impl TestClock {
    fn new() -> Rc<Self> {
        Rc::new(Self(Cell::new(waypost::SystemClock.now_ms())))
    }
    fn advance(&self, ms: i64) {
        self.0.set(self.0.get() + ms);
    }
}
impl Clock for TestClock {
    fn now_ms(&self) -> i64 {
        self.0.get()
    }
}
fn database(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open(f.dir.path().join("metadata.sqlite")).unwrap()
}
fn upload_request(token: &str, id: &str, bytes: &[u8]) -> Request {
    let mut r = Request::json(
        "PUT",
        &format!("/v0/uploads/{id}/content"),
        token,
        json!({}),
    );
    r.body = bytes.to_vec();
    r.headers.insert("content-type".into(), "text/plain".into());
    r
}
async fn prepare(f: &Fixture, key: &str, bytes: &[u8]) -> Value {
    let value=call(&f.app,"POST","/v0/uploads",&f.writer_token,json!({"request_id":key,"content_type":"text/plain","size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes))})).await;
    assert_eq!(
        f.app
            .handle(upload_request(
                &f.writer_token,
                value["upload_id"].as_str().unwrap(),
                bytes
            ))
            .await
            .status,
        200
    );
    value
}
#[tokio::test(flavor = "current_thread")]
async fn required_insert_conflict_rolls_back_the_entire_publication_and_can_recover() {
    let f = Fixture::new().await;
    f.send("first", "first").await;
    let db = database(&f);
    db.execute_batch(
        "CREATE UNIQUE INDEX injected_conflict ON message_recipients(space_id,actor_id)",
    )
    .unwrap();
    let input =
        json!({"request_id":"second","kind":"info","subject":"Second","body":{"text":"Second"}});
    let (status, error) = raw(
        &f.app,
        "POST",
        "/v0/messages",
        &f.writer_token,
        input.clone(),
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(error["error"]["code"], "INTERNAL_ERROR");
    assert_eq!(error["error"]["retryable"], false);
    assert!(!error.to_string().contains("message_recipients"));
    for table in ["messages", "feed_items", "threads", "message_recipients"] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "{table}");
    }
    db.execute_batch("DROP INDEX injected_conflict").unwrap();
    let recovered = call(
        &f.app,
        "POST",
        "/v0/messages",
        &f.writer_token,
        input.clone(),
    )
    .await;
    assert_eq!(
        recovered,
        call(&f.app, "POST", "/v0/messages", &f.writer_token, input).await
    );
    assert_eq!(f.inbox().await["items"].as_array().unwrap().len(), 2);
}
#[tokio::test(flavor = "current_thread")]
async fn cleanup_previews_expired_bytes_and_preserves_sealed_objects_and_messages() {
    let mut f = Fixture::new().await;
    let clock = TestClock::new();
    f.app = f.app.with_clock(clock.clone());
    let expired = prepare(&f, "expired", b"abandoned").await;
    let sealed = prepare(&f, "sealed", b"retained").await;
    let path = format!(
        "/v0/uploads/{}/commit",
        sealed["upload_id"].as_str().unwrap()
    );
    let blob = call(
        &f.app,
        "POST",
        &path,
        &f.writer_token,
        json!({"request_id":"seal-one"}),
    )
    .await;
    let another = call(
        &f.app,
        "POST",
        &path,
        &f.writer_token,
        json!({"request_id":"seal-two"}),
    )
    .await;
    assert_eq!(blob["blob_id"], another["blob_id"]);
    let message = f.send("published", "retained message").await;
    let object_path = f
        .dir
        .path()
        .join("objects")
        .join(format!("object_{}", expired["blob_id"].as_str().unwrap()));
    clock.advance(3_600_001);
    let preview = call(&f.app, "POST", CLEANUP, ADMIN, json!({"dry_run":true})).await;
    assert_eq!(preview["candidate_count"], 1);
    assert!(object_path.exists());
    let done = call(&f.app, "POST", CLEANUP, ADMIN, json!({"dry_run":false})).await;
    assert_eq!(done["removed"], 1);
    assert!(!object_path.exists());
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &format!(
                "/v0/uploads/{}/commit",
                expired["upload_id"].as_str().unwrap()
            ),
            &f.writer_token,
            json!({"request_id":"late-seal"})
        )
        .await
        .0,
        410
    );
    assert_eq!(
        f.app
            .handle(Request::json(
                "GET",
                &format!("/v0/blobs/{}/content", blob["blob_id"].as_str().unwrap()),
                &f.writer_token,
                json!({})
            ))
            .await
            .body,
        b"retained"
    );
    assert_eq!(
        call(
            &f.app,
            "GET",
            &format!("/v0/messages/{}", message["message_id"].as_str().unwrap()),
            &f.reader_token,
            json!({})
        )
        .await["body"]["text"],
        "retained message"
    );
}
#[tokio::test(flavor = "current_thread")]
async fn cleanup_is_admin_only_bounded_and_requires_an_explicit_mode() {
    let f = Fixture::new().await;
    for token in ["", &f.writer_token] {
        assert_eq!(
            raw(&f.app, "POST", CLEANUP, token, json!({"dry_run":false}))
                .await
                .0,
            401
        );
    }
    assert_eq!(raw(&f.app, "POST", CLEANUP, ADMIN, json!({})).await.0, 400);
    assert_eq!(
        raw(
            &f.app,
            "POST",
            CLEANUP,
            ADMIN,
            json!({"dry_run":false,"limit":9})
        )
        .await
        .0,
        413
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            CLEANUP,
            ADMIN,
            json!({"dry_run":false,"object_key":"arbitrary"})
        )
        .await
        .0,
        400
    );
    let login = f
        .app
        .handle(Request::json(
            "POST",
            "/admin/session",
            "",
            json!({"admin_secret":ADMIN}),
        ))
        .await;
    let mut request = Request::json("POST", CLEANUP, "", json!({"dry_run":false}));
    request.headers.remove("authorization");
    request.headers.insert(
        "cookie".into(),
        login.headers["set-cookie"]
            .split(';')
            .next()
            .unwrap()
            .into(),
    );
    assert_eq!(f.app.handle(request).await.status, 403);
}
type Hook = Rc<RefCell<Option<Box<dyn FnOnce(String) -> LocalBoxFuture<'static, ()>>>>>;
struct HookStore {
    store: FileStore,
    hook: Hook,
    fail_remove: Cell<bool>,
}
impl ObjectStore for HookStore {
    async fn create_only(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
        sha: &[u8],
    ) -> Result<ObjectInfo> {
        let hook = self.hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook(key.to_owned()).await;
        }
        self.store.create_only(key, bytes, content_type, sha).await
    }
    async fn inspect(&self, key: &str) -> Result<Option<ObjectInfo>> {
        self.store.inspect(key).await
    }
    async fn read(&self, key: &str) -> Result<Vec<u8>> {
        self.store.read(key).await
    }
    async fn remove(&self, key: &str) -> Result<()> {
        if self.fail_remove.replace(false) {
            return Err(Error::storage());
        }
        self.store.remove(key).await
    }
}
#[tokio::test(flavor = "current_thread")]
async fn cleanup_during_object_io_fences_publication_and_resweeps_a_late_write() {
    let f = Fixture::new().await;
    let clock = TestClock::new();
    let control = Rc::new(open(f.dir.path()).with_clock(clock.clone()));
    let control_hook = control.clone();
    let hook_clock = clock.clone();
    let hook: Hook = Rc::new(RefCell::new(Some(Box::new(move |_key| {
        Box::pin(async move {
            hook_clock.advance(24 * 3_600_000 + 1);
            let done = call(
                &control_hook,
                "POST",
                CLEANUP,
                ADMIN,
                json!({"dry_run":false}),
            )
            .await;
            assert_eq!(done["removed"], 1);
        })
    }))));
    let app = App::new(
        SqliteDb::open(f.dir.path().join("metadata.sqlite")).unwrap(),
        HookStore {
            store: FileStore::open(f.dir.path().join("objects")).unwrap(),
            hook,
            fail_remove: Cell::new(false),
        },
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    )
    .with_clock(clock.clone());
    let response=app.handle(Request::json("POST","/v0/messages",&f.writer_token,json!({"request_id":"late-body","kind":"info","subject":"Late","body":{"text":"private late bytes"}}))).await;
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 410);
    assert_eq!(value["error"]["code"], "SUBMISSION_EXPIRED");
    assert!(
        call(
            &control,
            "POST",
            "/v0/inbox/read",
            &f.reader_token,
            json!({"consumer_id":f.space["default_consumer_id"]})
        )
        .await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let key: String = database(&f)
        .query_row("SELECT object_key FROM object_writes", [], |r| r.get(0))
        .unwrap();
    let store = FileStore::open(f.dir.path().join("objects")).unwrap();
    assert!(store.inspect(&key).await.unwrap().is_some());
    clock.advance(3_600_001);
    assert_eq!(
        call(&control, "POST", CLEANUP, ADMIN, json!({"dry_run":false})).await["removed"],
        1
    );
    assert!(store.inspect(&key).await.unwrap().is_none());
    let status = call(
        &control,
        "POST",
        "/v0/submissions/status",
        &f.writer_token,
        json!({"request_id":"late-body","operation":"message.send"}),
    )
    .await;
    assert_eq!(status["error"]["code"], "SUBMISSION_EXPIRED");
}
#[tokio::test(flavor = "current_thread")]
async fn failed_object_deletion_can_resume_after_the_cleanup_lease_expires() {
    let mut f = Fixture::new().await;
    let clock = TestClock::new();
    f.app = f.app.with_clock(clock.clone());
    prepare(&f, "expired", b"retry deletion").await;
    clock.advance(3_600_001);
    let app = App::new(
        SqliteDb::open(f.dir.path().join("metadata.sqlite")).unwrap(),
        HookStore {
            store: FileStore::open(f.dir.path().join("objects")).unwrap(),
            hook: Rc::new(RefCell::new(None)),
            fail_remove: Cell::new(true),
        },
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    )
    .with_clock(clock.clone());
    assert_eq!(
        app.handle(Request::json(
            "POST",
            CLEANUP,
            ADMIN,
            json!({"dry_run":false})
        ))
        .await
        .status,
        503
    );
    assert_eq!(
        call(&f.app, "POST", CLEANUP, ADMIN, json!({"dry_run":false})).await["removed"],
        0
    );
    clock.advance(300_001);
    assert_eq!(
        call(&f.app, "POST", CLEANUP, ADMIN, json!({"dry_run":false})).await["removed"],
        1
    );
}
#[tokio::test(flavor = "current_thread")]
async fn independent_cursor_secret_survives_admin_rotation_and_detects_tampering() {
    let f = Fixture::new().await;
    f.send("first", "first").await;
    f.send("second", "second").await;
    let secret = "independent-cursor-secret-at-least-32-bytes";
    let before = open(f.dir.path()).with_cursor_secret(secret).unwrap();
    let first = call(
        &before,
        "POST",
        "/v0/inbox/read",
        &f.reader_token,
        json!({"consumer_id":f.space["default_consumer_id"],"limit":1}),
    )
    .await;
    let rotated = App::new(
        SqliteDb::open(f.dir.path().join("metadata.sqlite")).unwrap(),
        FileStore::open(f.dir.path().join("objects")).unwrap(),
        Config {
            admin_secret: "rotated-admin-secret-at-least-32-bytes".into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    )
    .with_cursor_secret(secret)
    .unwrap();
    let next=call(&rotated,"POST","/v0/inbox/read",&f.reader_token,json!({"consumer_id":f.space["default_consumer_id"],"cursor":first["next_cursor"],"limit":1})).await;
    assert_eq!(next["items"].as_array().unwrap().len(), 1);
    assert_ne!(next["items"][0]["item_id"], first["items"][0]["item_id"]);
    assert_eq!(raw(&rotated,"POST","/v0/inbox/read",&f.reader_token,json!({"consumer_id":f.space["default_consumer_id"],"cursor":format!("{}x",first["next_cursor"].as_str().unwrap())})).await.0,400);
}
#[tokio::test(flavor = "current_thread")]
async fn feed_positions_do_not_disclose_other_spaces_traffic() {
    let f = Fixture::new().await;
    f.send("first", "first").await;
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"other-space","display_name":"Other"}),
    )
    .await;
    let token=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",other["space_id"].as_str().unwrap()),ADMIN,json!({"request_id":"other-token","actor_id":other["default_actor_id"],"consumer_id":other["default_consumer_id"],"scopes":["message.send"],"resource_grants":{}})).await;
    call(
        &f.app,
        "POST",
        "/v0/messages",
        token["token"].as_str().unwrap(),
        json!({"request_id":"noise","kind":"info","subject":"Other","body":{"text":"Other"}}),
    )
    .await;
    f.send("second", "second").await;
    let page = f.inbox().await;
    assert_eq!(page["items"][0]["visible_seq"], "1");
    assert_eq!(page["items"][1]["visible_seq"], "2");
}
#[tokio::test(flavor = "current_thread")]
async fn malformed_blob_routes_do_not_acquire_binary_handlers() {
    let f = Fixture::new().await;
    for path in [
        "/v0/uploads/id/content/",
        "/v0/uploads//content",
        "/v0/blobs/id/content/",
        "/v0/blobs/id/extra/content",
    ] {
        let method = if path.contains("uploads") {
            "PUT"
        } else {
            "GET"
        };
        assert_eq!(
            raw(&f.app, method, path, &f.writer_token, json!({}))
                .await
                .0,
            404
        );
    }
}
