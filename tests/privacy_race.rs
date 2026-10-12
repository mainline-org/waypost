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
    on_read: bool,
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
        let hook = if !self.on_read && key.starts_with("body_message_") {
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
        let bytes = self.store.read(key).await?;
        let hook = if self.on_read {
            self.hook.borrow_mut().take()
        } else {
            None
        };
        if let Some(hook) = hook {
            hook().await;
        }
        Ok(bytes)
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
            on_read: false,
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

#[tokio::test(flavor = "current_thread")]
async fn a_policy_changed_during_attachment_read_cannot_cache_the_old_rejection() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new().await;
    let policy_path = format!("/admin/v0/spaces/{}/privacy", f.sid());
    call(&f.app, "PUT", &policy_path, ADMIN, json!({"expected_revision":0,"enabled":true,"mode":"block","all_agents":true,"actor_ids":[],"rules":[{"kind":"email"}]})).await;
    let content = b"A public attachment.";
    let upload = call(&f.app, "POST", "/v0/uploads", &f.writer_token, json!({"request_id":"race-upload","content_type":"text/plain","size_bytes":content.len(),"sha256":hex::encode(Sha256::digest(content))})).await;
    let mut put = Request::json(
        "PUT",
        &format!(
            "/v0/uploads/{}/content",
            upload["upload_id"].as_str().unwrap()
        ),
        &f.writer_token,
        json!({}),
    );
    put.headers
        .insert("content-type".into(), "text/plain".into());
    put.body = content.to_vec();
    assert_eq!(f.app.handle(put).await.status, 200);
    let sealed = call(
        &f.app,
        "POST",
        &format!(
            "/v0/uploads/{}/commit",
            upload["upload_id"].as_str().unwrap()
        ),
        &f.writer_token,
        json!({"request_id":"race-seal"}),
    )
    .await;
    let token = f.writer_token.clone();
    let control = Rc::new(f.app);
    let control_for_hook = control.clone();
    let hook: Hook = Rc::new(RefCell::new(Some(Box::new(move || {
        Box::pin(async move {
            call(&control_for_hook, "PUT", &policy_path, ADMIN, json!({"expected_revision":1,"enabled":false,"mode":"block","all_agents":true,"actor_ids":[],"rules":[{"kind":"email"}]})).await;
        })
    }))));
    let app = App::new(
        SqliteDb::open(f.dir.path().join("metadata.sqlite")).unwrap(),
        ChangingStore {
            store: FileStore::open(f.dir.path().join("objects")).unwrap(),
            hook,
            on_read: true,
        },
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    );
    let input = json!({"request_id":"old-policy-rejection","kind":"info","subject":"Contact","body":{"text":"private@example.org"},"attachment_ids":[sealed["blob_id"]]});
    let first = app
        .handle(Request::json("POST", "/v0/messages", &token, input.clone()))
        .await;
    let value: serde_json::Value = serde_json::from_slice(&first.body).unwrap();
    assert_eq!(value["error"]["code"], "SUBMISSION_PENDING");
    let status = app
        .handle(Request::json(
            "POST",
            "/v0/submissions/status",
            &token,
            json!({"operation":"message.send","request_id":"old-policy-rejection"}),
        ))
        .await;
    let status: serde_json::Value = serde_json::from_slice(&status.body).unwrap();
    assert_ne!(status["data"]["submission_state"], "failed");
    let retried = app
        .handle(Request::json("POST", "/v0/messages", &token, input))
        .await;
    assert_eq!(
        retried.status,
        200,
        "{}",
        String::from_utf8_lossy(&retried.body)
    );
}
