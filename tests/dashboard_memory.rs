mod common;
use common::*;
use futures_util::future::LocalBoxFuture;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use waypost::Request;
use waypost::{
    App, Clock, Config, Limits,
    error::Result,
    native::{FileStore, SqliteDb},
    storage::{ObjectInfo, ObjectStore},
};

async fn owner(f: &Fixture) -> String {
    call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/owner-credential", f.sid()),
        ADMIN,
        json!({"expected_version":0}),
    )
    .await["credential"]
        .as_str()
        .unwrap()
        .into()
}
async fn upload(f: &Fixture, key: &str, content: &[u8], content_type: &str) -> String {
    upload_as(f, key, content, content_type, &f.writer_token).await
}
async fn upload_as(
    f: &Fixture,
    key: &str,
    content: &[u8],
    content_type: &str,
    token: &str,
) -> String {
    let prepared=call(&f.app,"POST","/v0/uploads",token,json!({"request_id":key,"content_type":content_type,"size_bytes":content.len(),"sha256":hex::encode(Sha256::digest(content))})).await;
    let id = prepared["upload_id"].as_str().unwrap();
    let mut req = Request::json(
        "PUT",
        &format!("/v0/uploads/{id}/content"),
        token,
        json!({}),
    );
    req.body = content.to_vec();
    req.headers
        .insert("content-type".into(), content_type.into());
    assert_eq!(f.app.handle(req).await.status, 200);
    call(
        &f.app,
        "POST",
        &format!("/v0/uploads/{id}/commit"),
        token,
        json!({"request_id":format!("seal-{key}")}),
    )
    .await["blob_id"]
        .as_str()
        .unwrap()
        .into()
}

#[tokio::test(flavor = "current_thread")]
async fn final_edit_cannot_bind_another_actors_or_spaces_blob_or_invalid_text_bytes() {
    let mut f = Fixture::new().await;
    let credential = owner(&f).await;
    let original = upload(&f, "original", b"Keep original", "text/plain").await;
    let cid = collection(
        &f,
        json!([{ "item_id":"note","blob_id":original,"path":"note.md","metadata":{}}]),
    )
    .await;
    let direct = json!({"request_id":"direct-text","expected_revision":1,"item_id":"note","text":"Single-phase calls are not accepted"});
    assert_eq!(
        raw(&f.app, "POST", &edit_path(&f, &cid), &credential, direct)
            .await
            .0,
        400
    );
    let (status,error)=raw(&f.app,"POST",&edit_path(&f,&cid),&credential,json!({"request_id":"agent-owned","expected_revision":1,"item_id":"note","blob_id":original})).await;
    assert_eq!(status, 403);
    assert_eq!(error["error"]["code"], "FORBIDDEN");
    // Explicit fixture tokens exercise the existing public upload protocol. The
    // Dashboard implementation itself never creates or returns an Agent token.
    let owner_token=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",f.sid()),&credential,json!({"request_id":"fixture-owner-upload","actor_id":f.space["default_actor_id"],"consumer_id":f.space["default_consumer_id"],"scopes":["blob.write"],"resource_grants":{}})).await;
    let token = owner_token["token"].as_str().unwrap();
    let binary = upload_as(
        &f,
        "owner-binary",
        &[0, 255],
        "application/octet-stream",
        token,
    )
    .await;
    let invalid = upload_as(&f, "owner-invalid", &[255, 254], "text/plain", token).await;
    let wrong_mime = upload_as(&f, "owner-mime", b"Different type", "text/markdown", token).await;
    let large = upload_as(&f, "owner-large", &[b'x'; 33], "text/plain", token).await;
    for (name, bid) in [
        ("binary", binary),
        ("invalid", invalid),
        ("mime", wrong_mime),
    ] {
        assert_eq!(
            raw(
                &f.app,
                "POST",
                &edit_path(&f, &cid),
                &credential,
                json!({"request_id":name,"expected_revision":1,"item_id":"note","blob_id":bid})
            )
            .await
            .0,
            400
        );
    }
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"foreign-space","display_name":"Foreign"}),
    )
    .await;
    let other_token=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",other["space_id"].as_str().unwrap()),ADMIN,json!({"request_id":"foreign-token","actor_id":other["default_actor_id"],"consumer_id":other["default_consumer_id"],"scopes":["blob.write"],"resource_grants":{}})).await;
    let foreign = upload_as(
        &f,
        "foreign-text",
        b"Foreign private text",
        "text/plain",
        other_token["token"].as_str().unwrap(),
    )
    .await;
    assert_eq!(raw(&f.app,"POST",&edit_path(&f,&cid),&credential,json!({"request_id":"foreign-binding","expected_revision":1,"item_id":"note","blob_id":foreign})).await.0,409);
    f.app = f.app.with_limits(Limits {
        max_blob_bytes: 32,
        ..Default::default()
    });
    assert_eq!(raw(&f.app,"POST",&edit_path(&f,&cid),&credential,json!({"request_id":"oversized-binding","expected_revision":1,"item_id":"note","blob_id":large})).await.0,413);
    let current = call(&f.app, "POST", &read_path(&f, &cid), &credential, json!({})).await;
    assert_eq!(current["revision"], 1);
    assert_eq!(current["items"][0]["blob_id"], original);
}
async fn collection(f: &Fixture, items: Value) -> String {
    let c=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"collection","source_actor_id":f.writer["actor_id"],"scope":"notes","display_name":"Notes"})).await;
    let cid = c["collection_id"].as_str().unwrap();
    call(
        &f.app,
        "POST",
        &format!("/v0/memory/collections/{cid}/revisions"),
        &f.writer_token,
        json!({"request_id":"seed","expected_revision":0,"mode":"snapshot","items":items}),
    )
    .await;
    cid.into()
}
fn read_path(f: &Fixture, cid: &str) -> String {
    format!("/admin/v0/spaces/{}/memory/{cid}/read", f.sid())
}
fn edit_path(f: &Fixture, cid: &str) -> String {
    format!("/admin/v0/spaces/{}/memory/{cid}/edit", f.sid())
}
fn prepare_path(f: &Fixture, cid: &str) -> String {
    format!("{}/prepare", edit_path(f, cid))
}
async fn prepare_edit(f: &Fixture, cid: &str, credential: &str, input: Value) -> Value {
    let prepared = call(
        &f.app,
        "POST",
        &prepare_path(f, cid),
        credential,
        input.clone(),
    )
    .await;
    json!({"request_id":input["request_id"],"expected_revision":input["expected_revision"],"item_id":input["item_id"],"blob_id":prepared["blob_id"]})
}
fn blob_path(f: &Fixture, bid: &str) -> String {
    format!("/admin/v0/spaces/{}/blobs/{bid}/content", f.sid())
}

#[tokio::test(flavor = "current_thread")]
async fn owner_can_page_a_pinned_manifest_and_download_exact_binary_without_agent_admin_access() {
    let mut f = Fixture::new().await;
    f.app = f
        .app
        .with_encryption_keys(
            &json!({"active":"test","keys":{"test":"11".repeat(32)}}).to_string(),
            false,
        )
        .unwrap();
    let credential = owner(&f).await;
    let first = upload(
        &f,
        "binary",
        &[0, 255, 128, 10, 13],
        "application/octet-stream",
    )
    .await;
    let second = upload(&f, "text", b"Original text", "text/markdown").await;
    let cid=collection(&f,json!([
        {"item_id":"binary","blob_id":first,"path":"files/data.bin","metadata":{}},
        {"item_id":"note","blob_id":second,"path":"notes/test.md","metadata":{"origin":"synthetic"}}
    ])).await;
    let page = call(
        &f.app,
        "POST",
        &read_path(&f, &cid),
        &credential,
        json!({"limit":1,"include_tombstones":true}),
    )
    .await;
    assert_eq!(page["revision"], 1);
    assert_eq!(page["items"][0]["item_id"], "binary");
    assert_eq!(page["items"][0]["size_bytes"], 5);
    assert_eq!(page["items"][0]["content_type"], "application/octet-stream");
    assert_eq!(page["has_more"], true);
    let page2 = call(
        &f.app,
        "POST",
        &read_path(&f, &cid),
        &credential,
        json!({"revision":1,"cursor":page["next_cursor"],"limit":1,"include_tombstones":true}),
    )
    .await;
    assert_eq!(page2["items"][0]["path"], "notes/test.md");
    assert_eq!(page2["items"][0]["metadata"], json!({"origin":"synthetic"}));
    assert_eq!(page2["has_more"], false);
    let response = f
        .app
        .handle(Request::json(
            "GET",
            &blob_path(&f, &first),
            &credential,
            json!({}),
        ))
        .await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body, vec![0, 255, 128, 10, 13]);
    assert_eq!(response.headers["content-disposition"], "attachment");
    assert_eq!(response.headers["x-content-type-options"], "nosniff");
    assert!(response.headers["cache-control"].contains("no-store"));
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &read_path(&f, &cid),
            &f.writer_token,
            json!({})
        )
        .await
        .0,
        401
    );
    assert_eq!(
        f.app
            .handle(Request::json(
                "GET",
                &blob_path(&f, &first),
                &f.writer_token,
                json!({})
            ))
            .await
            .status,
        401
    );
}

#[tokio::test(flavor = "current_thread")]
async fn owner_text_edits_preserve_source_history_and_metadata_and_use_cas_and_idempotency() {
    let mut f = Fixture::new().await;
    f.app = f
        .app
        .with_encryption_keys(
            &json!({"active":"test","keys":{"test":"22".repeat(32)}}).to_string(),
            false,
        )
        .unwrap();
    let credential = owner(&f).await;
    let original = upload(&f, "text", b"Window seat", "text/markdown").await;
    let cid=collection(&f,json!([{ "item_id":"preferences","blob_id":original,"path":"travel/preferences.md","metadata":{"source":"user selected"}}])).await;
    let input = json!({"request_id":"edit-preferences","expected_revision":1,"item_id":"preferences","text":"Aisle seat"});
    let prepared = call(
        &f.app,
        "POST",
        &prepare_path(&f, &cid),
        &credential,
        input.clone(),
    )
    .await;
    assert_eq!(prepared["upload_state"], "sealed");
    assert_eq!(prepared["memory_published"], false);
    assert_eq!(
        call(&f.app, "POST", &read_path(&f, &cid), &credential, json!({})).await["revision"],
        1
    );
    assert_eq!(
        call(
            &f.app,
            "POST",
            &prepare_path(&f, &cid),
            &credential,
            input.clone()
        )
        .await,
        prepared
    );
    let final_input = json!({"request_id":input["request_id"],"expected_revision":1,"item_id":"preferences","blob_id":prepared["blob_id"]});
    let edited = call(
        &f.app,
        "POST",
        &edit_path(&f, &cid),
        &credential,
        final_input.clone(),
    )
    .await;
    assert_eq!(edited["revision"], 2);
    assert_eq!(edited["changed"], true);
    assert_eq!(
        call(
            &f.app,
            "POST",
            &edit_path(&f, &cid),
            &credential,
            final_input
        )
        .await,
        edited
    );
    let current = call(&f.app, "POST", &read_path(&f, &cid), &credential, json!({})).await;
    assert_eq!(current["revision"], 2);
    assert_eq!(current["source_actor_id"], f.writer["actor_id"]);
    assert_eq!(current["items"][0]["path"], "travel/preferences.md");
    assert_eq!(
        current["items"][0]["metadata"],
        json!({"source":"user selected"})
    );
    let downloaded = f
        .app
        .handle(Request::json(
            "GET",
            &blob_path(&f, edited["blob_id"].as_str().unwrap()),
            &credential,
            json!({}),
        ))
        .await;
    assert_eq!(downloaded.body, b"Aisle seat");
    let old = call(
        &f.app,
        "POST",
        &read_path(&f, &cid),
        &credential,
        json!({"revision":1}),
    )
    .await;
    assert_eq!(old["items"][0]["blob_id"], original);
    assert_eq!(
        f.app
            .handle(Request::json(
                "GET",
                &blob_path(&f, &original),
                &credential,
                json!({})
            ))
            .await
            .body,
        b"Window seat"
    );
    let revisions = call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/inspect", f.sid()),
        &credential,
        json!({"kind":"memory_revision","resource_id":cid}),
    )
    .await;
    assert_eq!(
        revisions["revisions"][0]["publisher_actor_id"],
        f.space["default_actor_id"]
    );
    assert_eq!(
        call(&f.app, "POST", &prepare_path(&f, &cid), &credential, input).await,
        prepared
    );
    let (status,error)=raw(&f.app,"POST",&prepare_path(&f,&cid),&credential,json!({"request_id":"stale","expected_revision":1,"item_id":"preferences","text":"Stale overwrite"})).await;
    assert_eq!(status, 409);
    assert_eq!(error["error"]["code"], "REVISION_CONFLICT");
    let (status,error)=raw(&f.app,"POST",&prepare_path(&f,&cid),&credential,json!({"request_id":"edit-preferences","expected_revision":1,"item_id":"preferences","text":"Changed retry"})).await;
    assert_eq!(status, 409);
    assert_eq!(error["error"]["code"], "IDEMPOTENCY_CONFLICT");
}

#[tokio::test(flavor = "current_thread")]
async fn owner_memory_boundaries_reject_cross_space_csrf_and_revoked_credentials_and_sessions() {
    let f = Fixture::new().await;
    let credential = owner(&f).await;
    let blob = upload(&f, "text", b"A private note", "text/plain").await;
    let cid = collection(
        &f,
        json!([{ "item_id":"note","blob_id":blob,"path":"note.md","metadata":{}}]),
    )
    .await;
    let login = f
        .app
        .handle(Request::json(
            "POST",
            "/admin/session",
            "",
            json!({"admin_secret":credential}),
        ))
        .await;
    assert_eq!(login.status, 200);
    let data: Value = serde_json::from_slice(&login.body).unwrap();
    let csrf = data["data"]["csrf_token"].as_str().unwrap();
    let cookie = login.headers["set-cookie"].split(';').next().unwrap();
    let input = prepare_edit(&f,&cid,&credential,json!({"request_id":"session-edit","expected_revision":1,"item_id":"note","text":"Updated private note"})).await;
    let mut missing_csrf = Request::json("POST", &edit_path(&f, &cid), "", input.clone());
    missing_csrf.headers.remove("authorization");
    missing_csrf.headers.insert("cookie".into(), cookie.into());
    assert_eq!(f.app.handle(missing_csrf).await.status, 403);
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"other","display_name":"Other"}),
    )
    .await;
    let other_sid = other["space_id"].as_str().unwrap();
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &format!("/admin/v0/spaces/{other_sid}/memory/{cid}/read"),
            &credential,
            json!({})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        f.app
            .handle(Request::json(
                "GET",
                &format!("/admin/v0/spaces/{other_sid}/blobs/{blob}/content"),
                &credential,
                json!({})
            ))
            .await
            .status,
        404
    );
    let mut foreign = Request::json("GET", &blob_path(&f, &blob), &credential, json!({}));
    foreign
        .headers
        .insert("origin".into(), "https://foreign.invalid".into());
    assert_eq!(f.app.handle(foreign).await.status, 403);
    let mut edit = Request::json("POST", &edit_path(&f, &cid), "", input);
    edit.headers.remove("authorization");
    edit.headers.insert("cookie".into(), cookie.into());
    edit.headers.insert("x-waypost-csrf".into(), csrf.into());
    assert_eq!(f.app.handle(edit).await.status, 200);
    call(
        &f.app,
        "DELETE",
        &format!("/admin/v0/spaces/{}/owner-credential", f.sid()),
        ADMIN,
        json!({}),
    )
    .await;
    assert_eq!(
        raw(&f.app, "POST", &read_path(&f, &cid), &credential, json!({}))
            .await
            .0,
        401
    );
    let mut request = Request::json("GET", &blob_path(&f, &blob), "", json!({}));
    request.headers.remove("authorization");
    request.headers.insert("cookie".into(), cookie.into());
    assert_eq!(f.app.handle(request).await.status, 401);
    assert_eq!(
        call(&f.app, "POST", &read_path(&f, &cid), ADMIN, json!({})).await["revision"],
        2
    );
}

type Hook = Rc<RefCell<Option<Box<dyn FnOnce() -> LocalBoxFuture<'static, ()>>>>>;
struct PublicationStore {
    store: FileStore,
    before_manifest: Hook,
}
impl ObjectStore for PublicationStore {
    async fn create_only(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
        sha: &[u8],
    ) -> Result<ObjectInfo> {
        if content_type == "application/json" {
            let hook = self.before_manifest.borrow_mut().take();
            if let Some(hook) = hook {
                hook().await;
            }
        }
        self.store.create_only(key, bytes, content_type, sha).await
    }
    async fn read(&self, key: &str) -> Result<Vec<u8>> {
        self.store.read(key).await
    }
    async fn inspect(&self, key: &str) -> Result<Option<ObjectInfo>> {
        self.store.inspect(key).await
    }
    async fn remove(&self, key: &str) -> Result<()> {
        self.store.remove(key).await
    }
}
#[tokio::test(flavor = "current_thread")]
async fn owner_revocation_during_manifest_io_prevents_the_edit_from_being_published() {
    let f = Fixture::new().await;
    let credential = owner(&f).await;
    let original = upload(&f, "text", b"Approved original", "text/plain").await;
    let cid = collection(
        &f,
        json!([{ "item_id":"note","blob_id":original,"path":"note.md","metadata":{}}]),
    )
    .await;
    let control = Rc::new(open(f.dir.path()));
    let revoke = control.clone();
    let sid = f.sid().to_string();
    let hook: Hook = Rc::new(RefCell::new(Some(Box::new(move || {
        Box::pin(async move {
            call(
                &revoke,
                "DELETE",
                &format!("/admin/v0/spaces/{sid}/owner-credential"),
                ADMIN,
                json!({}),
            )
            .await;
        })
    }))));
    let app = App::new(
        SqliteDb::open(f.dir.path().join("metadata.sqlite")).unwrap(),
        PublicationStore {
            store: FileStore::open(f.dir.path().join("objects")).unwrap(),
            before_manifest: hook.clone(),
        },
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    );
    let input=prepare_edit(&f,&cid,&credential,json!({"request_id":"revoke-during-io","expected_revision":1,"item_id":"note","text":"Must not publish"})).await;
    let response = app
        .handle(Request::json(
            "POST",
            &edit_path(&f, &cid),
            &credential,
            input,
        ))
        .await;
    assert_eq!(response.status, 401);
    assert!(
        hook.borrow().is_none(),
        "revocation must happen during manifest I/O"
    );
    let current = call(&control, "POST", &read_path(&f, &cid), ADMIN, json!({})).await;
    assert_eq!(current["revision"], 1);
    assert_eq!(current["items"][0]["blob_id"], original);
    let revisions = call(
        &control,
        "POST",
        &format!("/admin/v0/spaces/{}/inspect", f.sid()),
        ADMIN,
        json!({"kind":"memory_revision","resource_id":cid}),
    )
    .await;
    assert_eq!(revisions["revisions"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn export_pages_cover_more_than_one_maximum_page_and_stay_on_the_initial_revision() {
    let f = Fixture::new().await;
    let credential = owner(&f).await;
    let blob = upload(&f, "shared-bytes", b"Shared synthetic bytes", "text/plain").await;
    let items=(0..103).map(|n|json!({"item_id":format!("item-{n:03}"),"blob_id":blob,"path":format!("notes/{n:03}.md"),"metadata":{}})).collect::<Vec<_>>();
    let cid = collection(&f, json!(items)).await;
    let first = call(
        &f.app,
        "POST",
        &read_path(&f, &cid),
        &credential,
        json!({"limit":100}),
    )
    .await;
    assert_eq!(first["items"].as_array().unwrap().len(), 100);
    assert_eq!(first["has_more"], true);
    call(&f.app,"POST",&format!("/v0/memory/collections/{cid}/revisions"),&f.writer_token,json!({"request_id":"update-during-export","expected_revision":1,"mode":"delta","delete_ids":["item-102"]})).await;
    let last = call(
        &f.app,
        "POST",
        &read_path(&f, &cid),
        &credential,
        json!({"cursor":first["next_cursor"],"limit":100}),
    )
    .await;
    assert_eq!(last["revision"], 1);
    assert_eq!(last["items"].as_array().unwrap().len(), 3);
    assert_eq!(last["items"][2]["item_id"], "item-102");
    assert_eq!(last["items"][2]["size_bytes"], 22);
    assert_eq!(last["has_more"], false);
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &read_path(&f, &cid),
            &credential,
            json!({"revision":2,"cursor":first["next_cursor"]})
        )
        .await
        .0,
        400
    );
    let current = call(
        &f.app,
        "POST",
        &read_path(&f, &cid),
        &credential,
        json!({"revision":2,"include_tombstones":true,"limit":100}),
    )
    .await;
    let current_last = call(
        &f.app,
        "POST",
        &read_path(&f, &cid),
        &credential,
        json!({"revision":2,"include_tombstones":true,"cursor":current["next_cursor"]}),
    )
    .await;
    assert_eq!(current_last["items"][2]["deleted_in_revision"], 2);
}

struct TestClock(Cell<i64>);
impl Clock for TestClock {
    fn now_ms(&self) -> i64 {
        self.0.get()
    }
}
#[tokio::test(flavor = "current_thread")]
async fn text_edit_limits_and_expired_sessions_fail_without_replacing_the_document() {
    let mut f = Fixture::new().await;
    let clock = Rc::new(TestClock(Cell::new(0)));
    f.app = f.app.with_clock(clock.clone());
    let credential = owner(&f).await;
    let binary = upload(&f, "binary", &[0, 255], "application/octet-stream").await;
    let invalid = upload(&f, "invalid", &[255, 254], "text/plain").await;
    let text = upload(&f, "text", b"Retain me", "text/plain").await;
    let cid = collection(
        &f,
        json!([
            {"item_id":"binary","blob_id":binary,"path":"file.bin","metadata":{}},
            {"item_id":"invalid","blob_id":invalid,"path":"invalid.txt","metadata":{}},
            {"item_id":"text","blob_id":text,"path":"note.md","metadata":{}}
        ]),
    )
    .await;
    for item in ["binary", "invalid"] {
        assert_eq!(raw(&f.app,"POST",&prepare_path(&f,&cid),&credential,json!({"request_id":format!("edit-{item}"),"expected_revision":1,"item_id":item,"text":"Cannot replace binary"})).await.0,400);
    }
    assert_eq!(raw(&f.app,"POST",&prepare_path(&f,&cid),&credential,json!({"request_id":"change-path","expected_revision":1,"item_id":"text","text":"Unsafe path","path":"../escape"})).await.0,400);
    f.app = f.app.with_limits(Limits {
        max_blob_bytes: 32,
        ..Default::default()
    });
    assert_eq!(raw(&f.app,"POST",&prepare_path(&f,&cid),&credential,json!({"request_id":"too-large","expected_revision":1,"item_id":"text","text":"x".repeat(33)})).await.0,413);
    let login = f
        .app
        .handle(Request::json(
            "POST",
            "/admin/session",
            "",
            json!({"admin_secret":credential}),
        ))
        .await;
    let cookie = login.headers["set-cookie"].split(';').next().unwrap();
    clock.0.set(8 * 60 * 60 * 1000 + 1);
    let mut expired = Request::json("GET", &blob_path(&f, &text), "", json!({}));
    expired.headers.remove("authorization");
    expired.headers.insert("cookie".into(), cookie.into());
    assert_eq!(f.app.handle(expired).await.status, 401);
    let current = call(&f.app, "POST", &read_path(&f, &cid), &credential, json!({})).await;
    assert_eq!(current["revision"], 1);
    assert_eq!(current["items"][2]["blob_id"], text);
}
