// Exercise the public HTTP seam against SQLite while counting SQL statements at the
// storage boundary. Batch members each count separately, matching the D1 query budget.
use super::*;
use crate::native::{FileStore, SqliteDb};
use std::cell::Cell;
const ADMIN: &str = "synthetic-test-admin-secret-at-least-32-bytes";
struct CountDb {
    inner: SqliteDb,
    queries: Rc<Cell<usize>>,
}
impl Database for CountDb {
    async fn query(&self, statement: Statement) -> Result<Vec<Row>> {
        self.queries.set(self.queries.get() + 1);
        self.inner.query(statement).await
    }
    async fn batch(&self, statements: Vec<Statement>) -> Result<Vec<Vec<Row>>> {
        self.queries.set(self.queries.get() + statements.len());
        self.inner.batch(statements).await
    }
}
type TestApp = App<CountDb, FileStore>;
async fn call(app: &TestApp, method: &str, path: &str, token: &str, body: Value) -> Value {
    let response = app.handle(Request::json(method, path, token, body)).await;
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(response.status, 200, "{path}: {value}");
    value["data"].clone()
}
async fn measured(
    app: &TestApp,
    path: &str,
    cookie: &str,
    csrf: &str,
    body: Value,
    queries: &Cell<usize>,
    expected: u16,
) -> (Value, usize) {
    let mut request = Request::json("POST", path, "", body);
    request.headers.remove("authorization");
    request.headers.insert("cookie".into(), cookie.into());
    request.headers.insert("x-waypost-csrf".into(), csrf.into());
    queries.set(0);
    let response = app.handle(request).await;
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    let count = queries.get();
    assert_eq!(response.status, expected, "{path}: {value}");
    assert!(
        count <= 50,
        "{path} used {count} SQL statements, exceeding the D1 Free request limit"
    );
    (value["data"].clone(), count)
}
#[tokio::test(flavor = "current_thread")]
async fn two_phase_owner_edits_fit_the_worker_query_budget_at_collection_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let queries = Rc::new(Cell::new(0));
    let app = App::new(
        CountDb {
            inner: SqliteDb::open(dir.path().join("metadata.sqlite")).unwrap(),
            queries: queries.clone(),
        },
        FileStore::open(dir.path().join("objects")).unwrap(),
        Config {
            admin_secret: ADMIN.into(),
            public_origin: "http://127.0.0.1:8787".into(),
        },
    )
    .with_limits(Limits {
        max_json_bytes: 16384,
        max_blob_bytes: 256 * 1024,
        max_message_attachments: 4,
        max_inline_memory_changes: 8,
        max_manifest_memory_changes: 8,
        max_collection_items: 32,
        max_page_size: 20,
    })
    .with_encryption_keys(
        &json!({"active":"test","keys":{"test":"33".repeat(32)}}).to_string(),
        false,
    )
    .unwrap();
    let space = call(
        &app,
        "POST",
        "/admin/v0/spaces",
        ADMIN,
        json!({"request_id":"space","display_name":"Budget test"}),
    )
    .await;
    let sid = space["space_id"].as_str().unwrap();
    let base = format!("/admin/v0/spaces/{sid}");
    let writer = call(
        &app,
        "POST",
        &format!("{base}/actors"),
        ADMIN,
        json!({"request_id":"writer","display_name":"Writer","kind":"agent"}),
    )
    .await;
    let consumer = call(
        &app,
        "POST",
        &format!("{base}/consumers"),
        ADMIN,
        json!({"request_id":"consumer","actor_id":writer["actor_id"],"view":"inbox","filter":{}}),
    )
    .await;
    let issued=call(&app,"POST",&format!("{base}/tokens"),ADMIN,json!({"request_id":"token","actor_id":writer["actor_id"],"consumer_id":consumer["consumer_id"],"scopes":["blob.write","memory.write"],"resource_grants":{}})).await;
    let token = issued["token"].as_str().unwrap();
    let text = b"Original";
    let upload=call(&app,"POST","/v0/uploads",token,json!({"request_id":"upload","content_type":"text/plain","size_bytes":text.len(),"sha256":hex::encode(Sha256::digest(text))})).await;
    let uid = upload["upload_id"].as_str().unwrap();
    let mut put = Request::json(
        "PUT",
        &format!("/v0/uploads/{uid}/content"),
        token,
        json!({}),
    );
    put.body = text.to_vec();
    put.headers
        .insert("content-type".into(), "text/plain".into());
    assert_eq!(app.handle(put).await.status, 200);
    let sealed = call(
        &app,
        "POST",
        &format!("/v0/uploads/{uid}/commit"),
        token,
        json!({"request_id":"seal"}),
    )
    .await;
    let collection=call(&app,"POST",&format!("{base}/collections"),ADMIN,json!({"request_id":"collection","source_actor_id":writer["actor_id"],"scope":"budget","display_name":"Budget"})).await;
    let cid = collection["collection_id"].as_str().unwrap();
    for page in 0..4 {
        let items=(page*8..page*8+8).map(|n|json!({"item_id":format!("item-{n:02}"),"blob_id":sealed["blob_id"],"path":format!("{n:02}.txt"),"metadata":{}})).collect::<Vec<_>>();
        call(&app,"POST",&format!("/v0/memory/collections/{cid}/revisions"),token,json!({"request_id":format!("seed-{page}"),"expected_revision":page,"mode":"delta","items":items})).await;
    }
    let owner = call(
        &app,
        "POST",
        &format!("{base}/owner-credential"),
        ADMIN,
        json!({"expected_version":0}),
    )
    .await;
    let login = app
        .handle(Request::json(
            "POST",
            "/admin/session",
            "",
            json!({"admin_secret":owner["credential"]}),
        ))
        .await;
    let data: Value = serde_json::from_slice(&login.body).unwrap();
    let cookie = login.headers["set-cookie"].split(';').next().unwrap();
    let csrf = data["data"]["csrf_token"].as_str().unwrap();
    let edit = format!("{base}/memory/{cid}/edit");
    let prepare = format!("{edit}/prepare");
    let input = json!({"request_id":"change","expected_revision":4,"item_id":"item-00","text":"Replacement"});
    let (prepared, first_prepare) =
        measured(&app, &prepare, cookie, csrf, input.clone(), &queries, 200).await;
    let (again, retry_prepare) =
        measured(&app, &prepare, cookie, csrf, input.clone(), &queries, 200).await;
    assert_eq!(again, prepared);
    let final_input = json!({"request_id":"change","expected_revision":4,"item_id":"item-00","blob_id":prepared["blob_id"]});
    let (published, first_final) = measured(
        &app,
        &edit,
        cookie,
        csrf,
        final_input.clone(),
        &queries,
        200,
    )
    .await;
    assert_eq!(published["revision"], 5);
    let (again, retry_final) =
        measured(&app, &edit, cookie, csrf, final_input, &queries, 200).await;
    assert_eq!(again, published);
    let (_, stale_prepare) = measured(
        &app,
        &prepare,
        cookie,
        csrf,
        json!({"request_id":"stale","expected_revision":4,"item_id":"item-00","text":"Stale"}),
        &queries,
        409,
    )
    .await;
    let (_,stale_final)=measured(&app,&edit,cookie,csrf,json!({"request_id":"stale","expected_revision":4,"item_id":"item-00","blob_id":prepared["blob_id"]}),&queries,409).await;
    let (_,conflicting_retry)=measured(&app,&prepare,cookie,csrf,json!({"request_id":"change","expected_revision":4,"item_id":"item-01","text":"Replacement"}),&queries,409).await;
    eprintln!(
        "D1 SQL budget (32 items, encrypted, owner cookie): prepare={first_prepare}, prepare_retry={retry_prepare}, publish={first_final}, publish_retry={retry_final}, stale_prepare={stale_prepare}, stale_publish={stale_final}, conflicting_prepare_retry={conflicting_retry}"
    );
}
