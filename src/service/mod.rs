use crate::{
    domain::{FeedFilter, Principal, ResourceGrants},
    error::{Error, Result},
    http::{Request, Response},
    storage::{
        ObjectStore,
        driver::{Database, Row, Statement, Value as Sql, blob, integer, text},
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, rc::Rc};
mod admin;
mod blobs;
mod dashboard;
mod mailbox;
mod mcp;
mod memory;
mod privacy;
mod receipts;

#[derive(Clone)]
pub struct Config {
    pub admin_secret: String,
    pub public_origin: String,
}
#[derive(Clone, serde::Serialize)]
pub struct Limits {
    pub max_json_bytes: usize,
    pub max_blob_bytes: u64,
    pub max_message_attachments: usize,
    pub max_inline_memory_changes: usize,
    pub max_manifest_memory_changes: usize,
    pub max_collection_items: usize,
    pub max_page_size: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_json_bytes: 65536,
            max_blob_bytes: 8 * 1024 * 1024,
            max_message_attachments: 16,
            max_inline_memory_changes: 500,
            max_manifest_memory_changes: 10000,
            max_collection_items: 10000,
            max_page_size: 100,
        }
    }
}
pub trait Clock {
    fn now_ms(&self) -> i64;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
        }
        #[cfg(target_arch = "wasm32")]
        {
            js_sys::Date::now() as i64
        }
    }
}
pub struct App<D, O> {
    pub(crate) db: D,
    pub(crate) objects: O,
    pub(crate) config: Config,
    pub(crate) clock: Rc<dyn Clock>,
    pub(crate) limits: Limits,
}
// SQL execution is sealed inside this crate; callers use supported runtimes and domain HTTP/MCP operations.
#[allow(private_bounds)]
impl<D: Database, O: ObjectStore> App<D, O> {
    pub fn new(db: D, objects: O, config: Config) -> Self {
        Self {
            db,
            objects,
            config,
            clock: Rc::new(SystemClock),
            limits: Limits::default(),
        }
    }
    pub fn with_clock(mut self, clock: Rc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
    pub async fn handle(&self, request: Request) -> Response {
        if let Some(response) = self.dashboard_asset(&request) {
            return response;
        }
        if request.path == "/admin/session" {
            return self.admin_session(&request).await;
        }
        if request.path == "/mcp" {
            return self.mcp(&request).await;
        }
        let trace = match random_id("req") {
            Ok(v) => v,
            Err(e) => {
                return Response::json(
                    e.status(),
                    json!({"ok":false,"request_id":"unavailable","error":e.value()}),
                );
            }
        };
        let request_id = crate::json::parse(&request.body, self.limits.max_json_bytes)
            .ok()
            .and_then(|v| {
                v.get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or(trace);
        if request.method == "GET"
            && request.path.starts_with("/v0/blobs/")
            && request.path.ends_with("/content")
        {
            match self.blob_content(&request).await {
                Ok(response) => return response,
                Err(error) => {
                    return Response::json(
                        error.status(),
                        json!({"ok":false,"request_id":request_id,"error":error.value()}),
                    );
                }
            }
        }
        let result = self.route(&request).await;
        match result {
            Ok(data) => Response::json(200, json!({"ok":true,"request_id":request_id,"data":data})),
            Err(error) => Response::json(
                error.status(),
                json!({"ok":false,"request_id":request_id,"error":error.value()}),
            ),
        }
    }
    async fn route(&self, request: &Request) -> Result<Value> {
        if request.path == "/health" && request.method == "GET" {
            return Ok(json!({"status":"ok","api_version":"0.1"}));
        }
        if let Some(origin) = request.headers.get("origin")
            && origin != &self.config.public_origin
        {
            return Err(Error::forbidden());
        }
        if request.path.starts_with("/admin/v0/") {
            self.admin_access(request).await?;
            let value = crate::json::parse(&request.body, self.limits.max_json_bytes)?;
            return self.admin(request, value).await;
        }
        let principal = self.authenticate(request).await?;
        let parts = request
            .path
            .trim_matches('/')
            .split('/')
            .collect::<Vec<_>>();
        if request.method == "PUT"
            && parts.len() == 4
            && parts[0..2] == ["v0", "uploads"]
            && parts[3] == "content"
        {
            return self.blob_put(&principal, parts[2], request).await;
        }
        let value = crate::json::parse(&request.body, self.limits.max_json_bytes)?;
        if request.method == "POST"
            && parts.len() == 4
            && parts[0..2] == ["v0", "uploads"]
            && parts[3] == "commit"
        {
            return self.blob_commit(&principal, parts[2], value).await;
        }
        if request.method == "POST"
            && parts.len() == 5
            && parts[0..3] == ["v0", "memory", "collections"]
        {
            match parts[4] {
                "revisions" => return self.memory_publish(&principal, parts[3], value).await,
                "read" => return self.memory_read(&principal, parts[3], value).await,
                _ => return Err(Error::not_found()),
            }
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/v0/describe") => self.describe(&principal).await,
            ("POST", "/v0/uploads") => self.blob_prepare(&principal, value).await,
            ("GET", path) if path.starts_with("/v0/blobs/") => {
                self.blob_get(&principal, path.trim_start_matches("/v0/blobs/"))
                    .await
            }
            ("POST", "/v0/messages") => self.send(&principal, value).await,
            ("POST", "/v0/receipts") => self.ack(&principal, value).await,
            ("POST", "/v0/inbox/read") => self.inbox(&principal, value).await,
            ("POST", "/v0/submissions/status") => self.status(&principal, value).await,
            ("GET", path) if path.starts_with("/v0/messages/") => {
                self.message(&principal, path.trim_start_matches("/v0/messages/"))
                    .await
            }
            _ => Err(Error::not_found()),
        }
    }
    pub(crate) fn admin_auth(&self, request: &Request) -> Result<()> {
        let supplied = bearer(request)?;
        if self.config.admin_secret.len() < 32 {
            return Err(Error::storage());
        }
        let expected = Sha256::digest(self.config.admin_secret.as_bytes());
        let actual = Sha256::digest(supplied.as_bytes());
        let mut difference = 0u8;
        for (a, b) in expected.iter().zip(actual.iter()) {
            difference |= a ^ b;
        }
        if difference == 0 {
            Ok(())
        } else {
            Err(Error::unauthenticated())
        }
    }
    pub(crate) async fn authenticate(&self, request: &Request) -> Result<Principal> {
        let token = bearer(request)?;
        let digest = Sha256::digest(token.as_bytes()).to_vec();
        let now = self.clock.now_ms();
        let rows=self.db.query(Statement::new("SELECT t.token_id,t.space_id,t.actor_id,t.consumer_id,t.authority_version AS credential_version,a.authority_version AS actor_version,c.view,c.view_revision,s.default_actor_id FROM tokens t JOIN actors a ON a.space_id=t.space_id AND a.actor_id=t.actor_id JOIN consumers c ON c.space_id=t.space_id AND c.consumer_id=t.consumer_id AND c.actor_id=t.actor_id JOIN spaces s ON s.space_id=t.space_id WHERE t.token_digest=? AND t.revoked_at IS NULL AND (t.expires_at IS NULL OR t.expires_at>?) AND a.disabled=0 AND c.disabled=0",vec![digest.into(),now.into()])).await?;
        let row = rows.first().ok_or_else(Error::unauthenticated)?;
        let token_id = text(row, "token_id")?;
        let scopes = self
            .db
            .query(Statement::new(
                "SELECT scope FROM token_scopes WHERE token_id=? ORDER BY scope",
                vec![token_id.clone().into()],
            ))
            .await?
            .iter()
            .map(|r| text(r, "scope"))
            .collect::<Result<Vec<_>>>()?;
        let recipients = self
            .db
            .query(Statement::new(
                "SELECT actor_id FROM token_recipients WHERE token_id=? ORDER BY actor_id",
                vec![token_id.clone().into()],
            ))
            .await?
            .iter()
            .map(|r| text(r, "actor_id"))
            .collect::<Result<Vec<_>>>()?;
        let collections=self.db.query(Statement::new("SELECT collection_id,action FROM token_collections WHERE token_id=? ORDER BY collection_id,action",vec![token_id.clone().into()])).await?;
        let references = self
            .db
            .query(Statement::new(
                "SELECT blob_id FROM token_blob_grants WHERE token_id=? ORDER BY blob_id",
                vec![token_id.clone().into()],
            ))
            .await?
            .iter()
            .map(|r| text(r, "blob_id"))
            .collect::<Result<Vec<_>>>()?;
        let mut grants = ResourceGrants {
            recipient_ids: recipients,
            blob_reference_ids: references,
            ..Default::default()
        };
        for row in collections {
            if text(&row, "action")? == "read" {
                grants
                    .memory_read_collection_ids
                    .push(text(&row, "collection_id")?);
            } else {
                grants
                    .memory_write_collection_ids
                    .push(text(&row, "collection_id")?);
            }
        }
        let space_id = text(row, "space_id")?;
        let consumer_id = text(row, "consumer_id")?;
        let filters=self.db.query(Statement::new("SELECT dimension,value FROM consumer_filters WHERE space_id=? AND consumer_id=? ORDER BY dimension,value",vec![space_id.clone().into(),consumer_id.clone().into()])).await?;
        let mut filter = FeedFilter::default();
        for row in filters {
            let v = text(&row, "value")?;
            match text(&row, "dimension")?.as_str() {
                "producer_ids" => filter.producer_ids.push(v),
                "thread_ids" => filter.thread_ids.push(v),
                "item_types" => filter.item_types.push(v),
                "collection_ids" => filter.collection_ids.push(v),
                _ => return Err(Error::storage()),
            }
        }
        let grant_fingerprint = fingerprint(&json!({"scopes":scopes,"grants":grants}))?;
        let filter_fingerprint =
            fingerprint(&serde_json::to_value(&filter).map_err(|_| Error::storage())?)?;
        Ok(Principal {
            space_id,
            consumer_id,
            token_id,
            actor_id: text(row, "actor_id")?,
            actor_version: integer(row, "actor_version")?,
            credential_version: integer(row, "credential_version")?,
            view_revision: integer(row, "view_revision")?,
            default_actor_id: text(row, "default_actor_id")?,
            view: text(row, "view")?,
            scopes,
            grants,
            filter,
            grant_fingerprint,
            filter_fingerprint,
        })
    }
    pub(crate) fn authority_guard(&self, p: &Principal) -> (String, Vec<Sql>) {
        ("EXISTS(SELECT 1 FROM tokens t JOIN actors a ON a.space_id=t.space_id AND a.actor_id=t.actor_id JOIN consumers c ON c.space_id=t.space_id AND c.consumer_id=t.consumer_id WHERE t.token_id=? AND t.authority_version=? AND a.authority_version=? AND c.view_revision=? AND t.revoked_at IS NULL AND (t.expires_at IS NULL OR t.expires_at>?) AND a.disabled=0 AND c.disabled=0)".into(),vec![p.token_id.clone().into(),p.credential_version.into(),p.actor_version.into(),p.view_revision.into(),self.clock.now_ms().into()])
    }
    pub(crate) async fn current(&self, p: &Principal) -> Result<()> {
        let (guard, parameters) = self.authority_guard(p);
        let r = self
            .db
            .query(Statement::new(
                format!("SELECT 1 AS valid WHERE {guard}"),
                parameters,
            ))
            .await?;
        if r.is_empty() {
            Err(Error::unauthenticated())
        } else {
            Ok(())
        }
    }
    pub(crate) async fn describe(&self, p: &Principal) -> Result<Value> {
        self.current(p).await?;
        let mut collections = vec![];
        let mut more = false;
        if p.has("memory.read") || p.has("memory.write") {
            let rows=self.db.query(Statement::new("SELECT c.* FROM memory_collections c WHERE c.space_id=? AND (c.source_actor_id=? OR EXISTS(SELECT 1 FROM token_collections tc WHERE tc.token_id=? AND tc.space_id=c.space_id AND tc.collection_id=c.collection_id AND ((tc.action='read' AND ?=1) OR (tc.action='write' AND ?=1)))) ORDER BY c.created_at,c.collection_id LIMIT ?",vec![p.space_id.clone().into(),p.actor_id.clone().into(),p.token_id.clone().into(),p.has("memory.read").into(),p.has("memory.write").into(),((self.limits.max_page_size+1) as i64).into()])).await?;
            more = rows.len() > self.limits.max_page_size;
            for row in rows.iter().take(self.limits.max_page_size) {
                let id = text(row, "collection_id")?;
                let own = text(row, "source_actor_id")? == p.actor_id;
                collections.push(json!({"collection_id":id,"source_actor_id":text(row,"source_actor_id")?,"scope":text(row,"scope")?,"display_name":text(row,"display_name")?,"current_revision":integer(row,"current_revision")?,"can_read":p.has("memory.read") && (own || p.grants.memory_read_collection_ids.contains(&id)),"can_write":p.has("memory.write") && (own || p.grants.memory_write_collection_ids.contains(&id))}));
            }
        }
        self.current(p).await?;
        Ok(
            json!({"api_version":"0.1","space_id":p.space_id,"actor_id":p.actor_id,"consumer_id":p.consumer_id,"view_revision":p.view_revision,"default_recipient_id":p.default_actor_id,"allowed_recipient_ids":if p.grants.recipient_ids.is_empty(){vec![p.default_actor_id.clone()]}else{p.grants.recipient_ids.clone()},"scopes":p.scopes,"resource_grants":p.grants,"memory_collections":collections,"memory_collections_has_more":more,"limits":self.limits,"capabilities":{"privacy_filtering":true,"mailbox":true,"receipts":true,"blob_upload":true,"memory_snapshot":true,"memory_delta":true,"webhook_events":false,"oauth":false,"server_export":false}}),
        )
    }
    pub(crate) fn sign_cursor(&self, payload: &str) -> Result<String> {
        let mut m = Hmac::<Sha256>::new_from_slice(self.config.admin_secret.as_bytes())
            .map_err(|_| Error::storage())?;
        m.update(payload.as_bytes());
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(payload),
            URL_SAFE_NO_PAD.encode(m.finalize().into_bytes())
        ))
    }
    pub(crate) fn verify_cursor(&self, cursor: &str) -> Result<String> {
        let (body, tag) = cursor
            .split_once('.')
            .ok_or_else(|| Error::new("INVALID_CURSOR", "Invalid cursor."))?;
        let body = URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| Error::new("INVALID_CURSOR", "Invalid cursor."))?;
        let tag = URL_SAFE_NO_PAD
            .decode(tag)
            .map_err(|_| Error::new("INVALID_CURSOR", "Invalid cursor."))?;
        let mut m = Hmac::<Sha256>::new_from_slice(self.config.admin_secret.as_bytes())
            .map_err(|_| Error::storage())?;
        m.update(&body);
        m.verify_slice(&tag)
            .map_err(|_| Error::new("INVALID_CURSOR", "Invalid cursor."))?;
        String::from_utf8(body).map_err(|_| Error::new("INVALID_CURSOR", "Invalid cursor."))
    }
}
pub(crate) fn bearer(request: &Request) -> Result<String> {
    request
        .headers
        .get("authorization")
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| !s.is_empty() && s.len() <= 512)
        .map(str::to_owned)
        .ok_or_else(Error::unauthenticated)
}
pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0; N];
    getrandom::getrandom(&mut b).map_err(|_| Error::storage())?;
    Ok(b)
}
pub(crate) fn random_id(prefix: &str) -> Result<String> {
    Ok(format!("{prefix}_{}", hex::encode(random_bytes::<16>()?)))
}
pub(crate) fn fingerprint(v: &Value) -> Result<String> {
    Ok(hex::encode(Sha256::digest(crate::json::canonical(v)?)))
}
pub(crate) fn stamp(ms: i64) -> Result<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .ok_or_else(Error::storage)
}
pub(crate) fn uuid_uri() -> Result<String> {
    let mut b = random_bytes::<16>()?;
    b[6] = (b[6] & 15) | 64;
    b[8] = (b[8] & 63) | 128;
    let h = hex::encode(b);
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}
pub(crate) fn push_rows(
    table: &str,
    columns: &str,
    rows: Vec<Vec<Sql>>,
    guard: Option<(String, Vec<Sql>)>,
) -> Vec<Statement> {
    if rows.is_empty() {
        return vec![];
    }
    let width = rows[0].len();
    let reserved = guard.as_ref().map(|x| x.1.len()).unwrap_or(0);
    let chunk = ((100 - reserved) / width).max(1);
    let mut result = Vec::new();
    for rows in rows.chunks(chunk) {
        let mut parameters = Vec::new();
        let groups = rows
            .iter()
            .map(|r| {
                parameters.extend(r.iter().cloned());
                format!("({})", vec!["?"; width].join(","))
            })
            .collect::<Vec<_>>();
        let mut sql = format!(
            "INSERT OR IGNORE INTO {table}({columns}) SELECT * FROM (VALUES {})",
            groups.join(",")
        );
        if let Some((g, p)) = &guard {
            sql.push_str(&format!(" WHERE {g}"));
            parameters.extend(p.iter().cloned());
        }
        result.push(Statement::new(sql, parameters));
    }
    result
}
