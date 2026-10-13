mod common;
use common::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use waypost::Request;
fn keys(active: &str) -> String {
    json!({"active":active,"keys":{"v1":"11".repeat(32),"v2":"22".repeat(32)}}).to_string()
}
fn db(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open(f.dir.path().join("metadata.sqlite")).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn contact_migration_allows_removing_the_previous_encryption_key() {
    let mut f = Fixture::new().await;
    f.app = f.app.with_encryption_keys(&keys("v1"), false).unwrap();
    for id in ["first", "second"] {
        call(
            &f.app,
            "POST",
            "/admin/access-request",
            "",
            json!({"request_id":id,"email":format!("{id}@example.com")}),
        )
        .await;
    }
    f.app = f.app.with_encryption_keys(&keys("v2"), false).unwrap();
    let path = "/admin/v0/maintenance/encrypt";
    let preview = call(
        &f.app,
        "POST",
        path,
        ADMIN,
        json!({"kind":"access_requests","dry_run":true,"limit":1}),
    )
    .await;
    assert_eq!(preview["candidate_count"], 1);
    assert_eq!(preview["has_more"], true);
    let mut cursor = Value::Null;
    loop {
        let page = call(
            &f.app,
            "POST",
            path,
            ADMIN,
            json!({"kind":"access_requests","dry_run":false,"limit":1,"cursor":cursor}),
        )
        .await;
        assert_eq!(page["converted"], 1);
        if page["has_more"] != true {
            break;
        }
        cursor = page["next_cursor"].clone();
    }
    f.app = f
        .app
        .with_encryption_keys(
            &json!({"active":"v2","keys":{"v2":"22".repeat(32)}}).to_string(),
            false,
        )
        .unwrap();
    let listed = call(
        &f.app,
        "POST",
        "/admin/v0/access-requests",
        ADMIN,
        json!({}),
    )
    .await;
    assert_eq!(listed["items"][0]["email"], "first@example.com");
    assert_eq!(listed["items"][1]["email"], "second@example.com");
    assert_eq!(
        call(
            &f.app,
            "POST",
            path,
            ADMIN,
            json!({"kind":"access_requests","dry_run":false})
        )
        .await["candidate_count"],
        0
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            path,
            &f.writer_token,
            json!({"kind":"access_requests","dry_run":false})
        )
        .await
        .0,
        401
    );
}
async fn owner(f: &Fixture, space: &str) -> String {
    call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{space}/owner-credential"),
        ADMIN,
        json!({"expected_version":0}),
    )
    .await["credential"]
        .as_str()
        .unwrap()
        .into()
}
#[tokio::test(flavor = "current_thread")]
async fn encrypted_content_remains_usable_but_storage_contains_no_plaintext_content() {
    let mut f = Fixture::new().await;
    f.app = f.app.with_encryption_keys(&keys("v1"), false).unwrap();
    let bytes = b"private attachment content";
    let prepared=call(&f.app,"POST","/v0/uploads",&f.writer_token,json!({"request_id":"prepare","content_type":"text/plain","size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes)),"filename":"private-file.txt"})).await;
    let id = prepared["upload_id"].as_str().unwrap();
    let mut put = Request::json(
        "PUT",
        &format!("/v0/uploads/{id}/content"),
        &f.writer_token,
        json!({}),
    );
    put.body = bytes.to_vec();
    put.headers
        .insert("content-type".into(), "text/plain".into());
    assert_eq!(f.app.handle(put).await.status, 200);
    let sealed = call(
        &f.app,
        "POST",
        &format!("/v0/uploads/{id}/commit"),
        &f.writer_token,
        json!({"request_id":"seal"}),
    )
    .await;
    let bid = sealed["blob_id"].as_str().unwrap();
    let input = json!({"request_id":"encrypted-message","subject":"Private subject","kind":"info","body":{"text":"private body content"},"attachment_ids":[bid]});
    let sent = call(
        &f.app,
        "POST",
        "/v0/messages",
        &f.writer_token,
        input.clone(),
    )
    .await;
    assert_eq!(
        sent,
        call(&f.app, "POST", "/v0/messages", &f.writer_token, input).await
    );
    let message = call(
        &f.app,
        "GET",
        &format!("/v0/messages/{}", sent["message_id"].as_str().unwrap()),
        &f.reader_token,
        json!({}),
    )
    .await;
    assert_eq!(message["subject"], "Private subject");
    assert_eq!(message["body"]["text"], "private body content");
    assert_eq!(
        f.inbox().await["items"][0]["data"]["subject"],
        "Private subject"
    );
    let descriptor = call(
        &f.app,
        "GET",
        &format!("/v0/blobs/{bid}"),
        &f.reader_token,
        json!({}),
    )
    .await;
    assert_eq!(descriptor["filename"], "private-file.txt");
    assert_eq!(
        f.app
            .handle(Request::json(
                "GET",
                &format!("/v0/blobs/{bid}/content"),
                &f.reader_token,
                json!({})
            ))
            .await
            .body,
        bytes
    );
    let conn = db(&f);
    let (subject, body_key): (String, String) = conn
        .query_row("SELECT subject,body_ref FROM messages", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(subject.starts_with("wpenc1:"));
    let filename: String = conn
        .query_row("SELECT filename FROM blobs", [], |r| r.get(0))
        .unwrap();
    assert!(filename.starts_with("wpenc1:"));
    let cipher = std::fs::read(f.dir.path().join("objects").join(body_key)).unwrap();
    assert!(cipher.starts_with(b"WAYPOST1"));
    assert!(
        !cipher
            .windows(b"private body content".len())
            .any(|w| w == b"private body content")
    );
    let collection=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/collections",f.sid()),ADMIN,json!({"request_id":"collection","source_actor_id":f.writer["actor_id"],"display_name":"Memory","scope":"personal/default"})).await;
    let cid = collection["collection_id"].as_str().unwrap();
    let revision=call(&f.app,"POST",&format!("/v0/memory/collections/{cid}/revisions"),&f.writer_token,json!({"request_id":"publish","expected_revision":0,"mode":"snapshot","items":[{"item_id":"note","blob_id":bid,"path":"private/note.txt","metadata":{"secret":"private metadata"}}]})).await;
    assert_eq!(revision["revision"], 1);
    let state = call(
        &f.app,
        "POST",
        &format!("/v0/memory/collections/{cid}/read"),
        &f.writer_token,
        json!({}),
    )
    .await;
    assert_eq!(state["items"][0]["path"], "private/note.txt");
    assert_eq!(state["items"][0]["metadata"]["secret"], "private metadata");
    let (path, meta): (String, String) = conn
        .query_row("SELECT path,metadata FROM memory_revision_items", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(path.starts_with("wpenc1:"));
    assert!(meta.starts_with("wpenc1:"));
    let inspected = call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/inspect", f.sid()),
        ADMIN,
        json!({"kind":"memory","resource_id":cid}),
    )
    .await;
    assert_eq!(inspected["items"][0]["path"], "private/note.txt");
    f.app = f.app.with_encryption_keys(&keys("v2"), false).unwrap();
    let old = call(
        &f.app,
        "GET",
        &format!("/v0/messages/{}", sent["message_id"].as_str().unwrap()),
        &f.reader_token,
        json!({}),
    )
    .await;
    assert_eq!(old["body"]["text"], "private body content");
    f.send("new-key", "v2 content").await;
}
#[tokio::test(flavor = "current_thread")]
async fn copied_ciphertext_and_wrong_keys_fail_closed_without_exposing_content() {
    let mut f = Fixture::new().await;
    f.app = f.app.with_encryption_keys(&keys("v1"), false).unwrap();
    let first = f.send("first", "First private body").await;
    let second = f.send("second", "Second private body").await;
    let conn = db(&f);
    let key = |id: &Value| -> String {
        conn.query_row(
            "SELECT body_ref FROM messages WHERE message_id=?",
            [id.as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap()
    };
    let first_key = key(&first["message_id"]);
    let second_key = key(&second["message_id"]);
    let path = f.dir.path().join("objects");
    let cipher = std::fs::read(path.join(&first_key)).unwrap();
    std::fs::write(path.join(&second_key), &cipher).unwrap();
    let result = raw(
        &f.app,
        "GET",
        &format!("/v0/messages/{}", second["message_id"].as_str().unwrap()),
        &f.reader_token,
        json!({}),
    )
    .await;
    assert_eq!(result.0, 500);
    assert_eq!(result.1["error"]["retryable"], false);
    assert!(!result.1.to_string().contains("First private"));
    f.app = f
        .app
        .with_encryption_keys(
            &json!({"active":"v1","keys":{"v1":"33".repeat(32)}}).to_string(),
            false,
        )
        .unwrap();
    assert_eq!(
        raw(
            &f.app,
            "GET",
            &format!("/v0/messages/{}", first["message_id"].as_str().unwrap()),
            &f.reader_token,
            json!({})
        )
        .await
        .0,
        500
    );
}
#[tokio::test(flavor = "current_thread")]
async fn user_credentials_sessions_registry_and_revocation_are_space_scoped() {
    let f = Fixture::new().await;
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"other","display_name":"Other user"}),
    )
    .await;
    let other_id = other["space_id"].as_str().unwrap();
    let credential = owner(&f, f.sid()).await;
    let other_credential = owner(&f, other_id).await;
    let list = call(
        &f.app,
        "POST",
        "/admin/v0/registry",
        &credential,
        json!({"kind":"spaces"}),
    )
    .await;
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["space_id"], f.sid());
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/v0/registry",
            &credential,
            json!({"kind":"actors","space_id":other_id})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &format!("/admin/v0/spaces/{other_id}/inspect"),
            &credential,
            json!({"kind":"messages"})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/v0/spaces",
            &credential,
            json!({"request_id":"escalate","display_name":"Escalation"})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            &format!("/admin/v0/spaces/{}/owner-credential", f.sid()),
            &credential,
            json!({"expected_version":1})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "POST",
            "/admin/v0/maintenance/cleanup",
            &credential,
            json!({"dry_run":false})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "DELETE",
            &format!(
                "/admin/v0/spaces/{other_id}/tokens/{}",
                f.writer["actor_id"].as_str().unwrap()
            ),
            &credential,
            json!({})
        )
        .await
        .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "DELETE",
            &format!(
                "/admin/v0/tokens/{}",
                f.writer["actor_id"].as_str().unwrap()
            ),
            &credential,
            json!({})
        )
        .await
        .0,
        404
    );
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
    let session: Value = serde_json::from_slice(&login.body).unwrap();
    assert_eq!(session["data"]["operator"], false);
    let mut req = Request::json("POST", "/admin/v0/registry", "", json!({"kind":"spaces"}));
    req.headers.remove("authorization");
    req.headers.insert(
        "cookie".into(),
        login.headers["set-cookie"]
            .split(';')
            .next()
            .unwrap()
            .into(),
    );
    req.headers.insert(
        "x-waypost-csrf".into(),
        session["data"]["csrf_token"].as_str().unwrap().into(),
    );
    let own = f.app.handle(req).await;
    assert_eq!(own.status, 200);
    let own: Value = serde_json::from_slice(&own.body).unwrap();
    assert_eq!(own["data"]["items"][0]["space_id"], f.sid());
    let rotated = call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/owner-credential", f.sid()),
        ADMIN,
        json!({"expected_version":1}),
    )
    .await;
    assert_eq!(
        raw(&f.app, "GET", "/admin/v0/spaces", &credential, json!({}))
            .await
            .0,
        401
    );
    let mut old_session = Request::json("GET", "/admin/session", "", json!({}));
    old_session.headers.remove("authorization");
    old_session.headers.insert(
        "cookie".into(),
        login.headers["set-cookie"]
            .split(';')
            .next()
            .unwrap()
            .into(),
    );
    assert_eq!(f.app.handle(old_session).await.status, 401);
    assert_eq!(
        call(
            &f.app,
            "GET",
            "/admin/v0/spaces",
            &other_credential,
            json!({})
        )
        .await["spaces"][0]["space_id"],
        other_id
    );
    let own_tokens = call(
        &f.app,
        "POST",
        &format!("/admin/v0/spaces/{}/inspect", f.sid()),
        rotated["credential"].as_str().unwrap(),
        json!({"kind":"tokens"}),
    )
    .await;
    let tid = own_tokens["items"][0]["token_id"].as_str().unwrap();
    call(
        &f.app,
        "DELETE",
        &format!("/admin/v0/spaces/{}/tokens/{tid}", f.sid()),
        rotated["credential"].as_str().unwrap(),
        json!({}),
    )
    .await;
}
#[tokio::test(flavor = "current_thread")]
async fn strict_encryption_rejects_legacy_plaintext_until_explicitly_enabled() {
    let mut f = Fixture::new().await;
    let original = f.send("plain", "legacy plaintext").await;
    f.app = f.app.with_encryption_keys(&keys("v1"), false).unwrap();
    let path = format!("/v0/messages/{}", original["message_id"].as_str().unwrap());
    assert_eq!(
        raw(&f.app, "GET", &path, &f.reader_token, json!({}))
            .await
            .0,
        500
    );
    f.app = f.app.with_encryption_keys(&keys("v1"), true).unwrap();
    assert_eq!(
        call(&f.app, "GET", &path, &f.reader_token, json!({})).await["body"]["text"],
        "legacy plaintext"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_migration_is_previewable_repeatable_and_preserves_exact_content() {
    let mut f = Fixture::new().await;
    let original = f.send("legacy", "legacy bytes").await;
    let old_key: String = db(&f)
        .query_row("SELECT body_ref FROM messages", [], |r| r.get(0))
        .unwrap();
    f.app = f.app.with_encryption_keys(&keys("v1"), false).unwrap();
    let route = "/admin/v0/maintenance/encrypt";
    assert_eq!(
        call(
            &f.app,
            "POST",
            route,
            ADMIN,
            json!({"kind":"objects","dry_run":true})
        )
        .await["candidate_count"],
        1
    );
    assert!(
        std::fs::read(f.dir.path().join("objects").join(&old_key))
            .unwrap()
            .starts_with(b"{")
    );
    for kind in ["objects", "messages", "uploads", "blobs", "memory_items"] {
        call(
            &f.app,
            "POST",
            route,
            ADMIN,
            json!({"kind":kind,"dry_run":false}),
        )
        .await;
        assert_eq!(
            call(
                &f.app,
                "POST",
                route,
                ADMIN,
                json!({"kind":kind,"dry_run":false})
            )
            .await["candidate_count"],
            0
        );
    }
    let changed_key: String = db(&f)
        .query_row("SELECT body_ref FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_ne!(old_key, changed_key);
    assert_eq!(
        call(
            &f.app,
            "GET",
            &format!("/v0/messages/{}", original["message_id"].as_str().unwrap()),
            &f.reader_token,
            json!({})
        )
        .await["body"]["text"],
        "legacy bytes"
    );
    assert_eq!(
        call(
            &f.app,
            "POST",
            "/admin/v0/maintenance/cleanup",
            ADMIN,
            json!({"dry_run":false})
        )
        .await["removed"],
        1
    );
    assert!(!f.dir.path().join("objects").join(old_key).exists());
    assert!(f.dir.path().join("objects").join(changed_key).exists());
}

#[tokio::test(flavor = "current_thread")]
async fn another_users_ciphertext_cannot_be_read_or_relocated_into_the_current_space() {
    let mut f = Fixture::new().await;
    f.app = f.app.with_encryption_keys(&keys("v1"), false).unwrap();
    let own = f.send("own", "Owner one content").await;
    let other = call(
        &f.app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"another","display_name":"Another user"}),
    )
    .await;
    let token=call(&f.app,"POST",&format!("/admin/v0/spaces/{}/tokens",other["space_id"].as_str().unwrap()),ADMIN,json!({"request_id":"another-token","actor_id":other["default_actor_id"],"consumer_id":other["default_consumer_id"],"scopes":["message.send","message.read"],"resource_grants":{}})).await;
    let other_token = token["token"].as_str().unwrap();
    let sent=call(&f.app,"POST","/v0/messages",other_token,json!({"request_id":"another-message","kind":"info","subject":"Another","body":{"text":"Owner two content"}})).await;
    let other_path = format!("/v0/messages/{}", sent["message_id"].as_str().unwrap());
    assert_eq!(
        raw(&f.app, "GET", &other_path, &f.reader_token, json!({}))
            .await
            .0,
        404
    );
    assert_eq!(
        raw(
            &f.app,
            "GET",
            &format!("/v0/messages/{}", own["message_id"].as_str().unwrap()),
            other_token,
            json!({})
        )
        .await
        .0,
        404
    );
    let conn = db(&f);
    let get_key = |id: &Value| -> String {
        conn.query_row(
            "SELECT body_ref FROM messages WHERE message_id=?",
            [id.as_str().unwrap()],
            |r| r.get(0),
        )
        .unwrap()
    };
    let own_key = get_key(&own["message_id"]);
    let other_key = get_key(&sent["message_id"]);
    let folder = f.dir.path().join("objects");
    std::fs::write(
        folder.join(other_key),
        std::fs::read(folder.join(own_key)).unwrap(),
    )
    .unwrap();
    let (status, error) = raw(&f.app, "GET", &other_path, other_token, json!({})).await;
    assert_eq!(status, 500);
    assert_eq!(error["error"]["code"], "INTERNAL_ERROR");
    assert!(!error.to_string().contains("Owner one content"));
}
#[tokio::test(flavor = "current_thread")]
async fn offline_cli_migration_preserves_data_and_respects_the_runtime_lock() {
    let f = Fixture::new().await;
    let sent = f.send("cli-legacy", "CLI legacy data").await;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(f.dir.path().join("runtime-lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let run = || {
        std::process::Command::new(env!("CARGO_BIN_EXE_waypost"))
            .args(["encrypt", "--data"])
            .arg(f.dir.path())
            .env("WAYPOST_ADMIN_SECRET", ADMIN)
            .env("WAYPOST_ENCRYPTION_KEYS", keys("v1"))
            .output()
            .unwrap()
    };
    assert!(!run().status.success());
    drop(lock);
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let app = open(f.dir.path())
        .with_encryption_keys(&keys("v1"), false)
        .unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/v0/messages/{}", sent["message_id"].as_str().unwrap()),
            &f.reader_token,
            json!({})
        )
        .await["body"]["text"],
        "CLI legacy data"
    );
}
