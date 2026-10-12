mod common;
use common::*;
use futures_util::future::LocalBoxFuture;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use waypost::{
    App, Config, Request,
    error::Result,
    native::{FileStore, SqliteDb},
    storage::{ObjectInfo, ObjectStore},
};
type Hook = Rc<RefCell<Option<Box<dyn FnOnce() -> LocalBoxFuture<'static, ()>>>>>;
struct ChangingStore {
    store: FileStore,
    hook: Hook,
}
impl ObjectStore for ChangingStore {
    async fn create_only(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
        sha: &[u8],
    ) -> Result<ObjectInfo> {
        let result = self
            .store
            .create_only(key, bytes, content_type, sha)
            .await?;
        let hook = if key.starts_with("body_message_") {
            self.hook.borrow_mut().take()
        } else {
            None
        };
        if let Some(hook) = hook {
            hook().await
        }
        Ok(result)
    }
    async fn inspect(&self, key: &str) -> Result<Option<ObjectInfo>> {
        self.store.inspect(key).await
    }
    async fn read(&self, key: &str) -> Result<Vec<u8>> {
        self.store.read(key).await
    }
    async fn remove(&self, key: &str) -> Result<()> {
        self.store.remove(key).await
    }
}
#[tokio::test(flavor = "current_thread")]
async fn a_policy_changed_after_body_preparation_cannot_publish_the_old_unfiltered_message() {
    let f = Fixture::new().await;
    let token = f.writer_token.clone();
    let reader = f.reader_token.clone();
    let sid = f.sid().to_owned();
    let actor = f.writer["actor_id"].clone();
    let control = Rc::new(f.app);
    let control_for_hook = control.clone();
    let hook: Hook = Rc::new(RefCell::new(Some(Box::new(move || {
        Box::pin(async move {
            call(&control_for_hook,"PUT",&format!("/admin/v0/spaces/{sid}/privacy"),ADMIN,json!({"expected_revision":0,"enabled":true,"mode":"redact","all_agents":false,"actor_ids":[actor],"rules":[{"kind":"email"}]})).await;
        })
    }))));
    let app = App::new(
        SqliteDb::open(f.dir.path().join("metadata.sqlite")).unwrap(),
        ChangingStore {
            store: FileStore::open(f.dir.path().join("objects")).unwrap(),
            hook,
        },
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    );
    let input = json!({"request_id":"policy-race","kind":"info","subject":"Contact","body":{"text":"private@example.org"}});
    let r = app
        .handle(Request::json("POST", "/v0/messages", &token, input.clone()))
        .await;
    assert_eq!(r.status, 409);
    let r = app
        .handle(Request::json("POST", "/v0/messages", &token, input))
        .await;
    assert_eq!(r.status, 200);
    let value: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
    let id = value["data"]["message_id"].as_str().unwrap();
    let msg = call(
        &control,
        "GET",
        &format!("/v0/messages/{id}"),
        &reader,
        json!({}),
    )
    .await;
    assert_eq!(msg["body"]["text"], "[REDACTED]");
}
