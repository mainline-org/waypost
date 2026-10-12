use crate::{
    App, Config, Limits, Request, Response,
    error::{Error, Result},
    storage::{
        ObjectInfo, ObjectStore,
        driver::{Database, Row, Statement, Value as Sql, blob, integer, text},
    },
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wasm_bindgen::JsValue;
use worker::{
    Bucket, Conditional, Context, D1DatabaseSession, D1PreparedStatement, D1Result, Env,
    FixedLengthStream, HttpMetadata, event,
};

struct D1Store {
    session: D1DatabaseSession,
}
impl D1Store {
    fn prepare(&self, statement: Statement) -> Result<D1PreparedStatement> {
        if statement.parameters.len() > 100 || statement.sql.len() > 100_000 {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Statement exceeds the D1 deployment limit.",
            ));
        }
        let parameters = statement
            .parameters
            .into_iter()
            .map(|v| match v {
                Sql::Null => JsValue::NULL,
                Sql::Text(s) => JsValue::from_str(&s),
                Sql::Integer(n) => {
                    if n.unsigned_abs() <= 9_007_199_254_740_991 {
                        JsValue::from_f64(n as f64)
                    } else {
                        JsValue::from_str(&n.to_string())
                    }
                }
                Sql::Blob(bytes) => js_sys::Uint8Array::from(bytes.as_slice()).into(),
            })
            .collect::<Vec<_>>();
        self.session
            .prepare(statement.sql)
            .bind(&parameters)
            .map_err(|_| Error::storage())
    }
}
fn result_rows(result: D1Result) -> Result<Vec<Row>> {
    if !result.success() {
        return Err(Error::storage());
    }
    let rows = result
        .results::<BTreeMap<String, Value>>()
        .map_err(|_| Error::storage())?;
    rows.into_iter()
        .map(|r| {
            r.into_iter()
                .map(|(name, value)| {
                    let value = match value {
                        Value::Null => Sql::Null,
                        Value::String(s) => Sql::Text(s),
                        Value::Number(n) => Sql::Integer(n.as_i64().ok_or_else(Error::storage)?),
                        Value::Array(a) => Sql::Blob(
                            a.into_iter()
                                .map(|v| {
                                    v.as_u64()
                                        .filter(|v| *v <= 255)
                                        .map(|v| v as u8)
                                        .ok_or_else(Error::storage)
                                })
                                .collect::<Result<Vec<_>>>()?,
                        ),
                        _ => return Err(Error::storage()),
                    };
                    Ok((name, value))
                })
                .collect()
        })
        .collect()
}
impl Database for D1Store {
    async fn query(&self, statement: Statement) -> Result<Vec<Row>> {
        result_rows(
            self.prepare(statement)?
                .all()
                .await
                .map_err(|_| Error::storage())?,
        )
    }
    async fn batch(&self, statements: Vec<Statement>) -> Result<Vec<Vec<Row>>> {
        let statements = statements
            .into_iter()
            .map(|s| self.prepare(s))
            .collect::<Result<Vec<_>>>()?;
        self.session
            .batch(statements)
            .await
            .map_err(|_| Error::storage())?
            .into_iter()
            .map(result_rows)
            .collect()
    }
}
struct R2Store {
    bucket: Bucket,
}
fn object_info(object: &worker::Object) -> Result<ObjectInfo> {
    Ok(ObjectInfo {
        size: object.size(),
        sha256: object.checksum().sha256.ok_or_else(Error::storage)?,
        content_type: object
            .http_metadata()
            .content_type
            .ok_or_else(Error::storage)?,
    })
}
impl R2Store {
    async fn put(
        &self,
        key: &str,
        data: worker::Data,
        content_type: &str,
        sha: &[u8],
        size: u64,
    ) -> Result<ObjectInfo> {
        let object = self
            .bucket
            .put(key, data)
            .http_metadata(HttpMetadata {
                content_type: Some(content_type.into()),
                ..Default::default()
            })
            .only_if(Conditional {
                etag_does_not_match: Some("*".into()),
                ..Default::default()
            })
            .sha256(sha.to_vec())
            .execute()
            .await
            .map_err(|error| {
                if error.to_string().to_ascii_lowercase().contains("checksum") {
                    Error::new("CHECKSUM_MISMATCH", "R2 rejected the byte checksum.")
                } else {
                    Error::storage()
                }
            })?;
        let info = if let Some(object) = object {
            object_info(&object)?
        } else {
            self.inspect(key).await?.ok_or_else(Error::storage)?
        };
        if info.size != size || info.sha256 != sha || info.content_type != content_type {
            return Err(Error::new(
                "UPLOAD_CONTENT_CONFLICT",
                "Existing immutable object differs from the upload.",
            ));
        }
        Ok(info)
    }
}
impl ObjectStore for R2Store {
    async fn create_only(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
        sha: &[u8],
    ) -> Result<ObjectInfo> {
        self.put(
            key,
            bytes.to_vec().into(),
            content_type,
            sha,
            bytes.len() as u64,
        )
        .await
    }
    async fn inspect(&self, key: &str) -> Result<Option<ObjectInfo>> {
        self.bucket
            .head(key)
            .await
            .map_err(|_| Error::storage())?
            .as_ref()
            .map(object_info)
            .transpose()
    }
    async fn read(&self, key: &str) -> Result<Vec<u8>> {
        let object = self
            .bucket
            .get(key)
            .execute()
            .await
            .map_err(|_| Error::storage())?
            .ok_or_else(Error::not_found)?;
        object
            .body()
            .ok_or_else(Error::storage)?
            .bytes()
            .await
            .map_err(|_| Error::storage())
    }
    async fn remove(&self, key: &str) -> Result<()> {
        self.bucket.delete(key).await.map_err(|_| Error::storage())
    }
}
impl App<D1Store, R2Store> {
    async fn streaming_upload(
        &self,
        request: &Request,
        raw: &mut worker::Request,
        id: &str,
    ) -> Result<Value> {
        if request
            .headers
            .get("origin")
            .is_some_and(|s| s != &self.config.public_origin)
        {
            return Err(Error::forbidden());
        }
        let p = self.authenticate(request).await?;
        p.require("blob.write")?;
        let row = self.upload(&p, id).await?;
        if text(&row, "state")? == "sealed" {
            return Err(Error::new("UPLOAD_SEALED", "Upload is sealed."));
        }
        if integer(&row, "expires_at")? <= self.clock.now_ms() {
            return Err(Error::new("UPLOAD_EXPIRED", "Upload expired."));
        }
        let size = integer(&row, "size_bytes")? as u64;
        let content_type = text(&row, "content_type")?;
        if size > self.limits.max_blob_bytes {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Blob exceeds Worker deployment limit.",
            ));
        }
        if request.headers.get("content-type") != Some(&content_type)
            || request
                .headers
                .get("content-length")
                .is_some_and(|v| v.parse::<u64>().ok() != Some(size))
        {
            return Err(Error::new(
                "INVALID_BLOB",
                "Size or content type does not match.",
            ));
        }
        let key = text(&row, "object_key")?;
        let existing = self.objects.inspect(&key).await?;
        let data = if raw.inner().body().is_none() {
            if size != 0 {
                return Err(Error::new("INVALID_BLOB", "Upload body is missing."));
            }
            worker::Data::Bytes(vec![])
        } else {
            FixedLengthStream::wrap(
                raw.stream()
                    .map_err(|_| Error::new("INVALID_BLOB", "Invalid upload stream."))?,
                size,
            )
            .into()
        };
        self.objects
            .put(&key, data, &content_type, &blob(&row, "sha256")?, size)
            .await?;
        self.current(&p).await?;
        let after = self.upload(&p, id).await?;
        if text(&after, "state")? == "sealed" {
            return Err(Error::new("UPLOAD_SEALED", "Upload is sealed."));
        }
        if integer(&after, "expires_at")? <= self.clock.now_ms() {
            return Err(Error::new("UPLOAD_EXPIRED", "Upload expired."));
        }
        Ok(json!({"upload_id":id,"already_uploaded":existing.is_some()}))
    }
    async fn streaming_download(&self, request: &Request, id: &str) -> Result<worker::Response> {
        if request
            .headers
            .get("origin")
            .is_some_and(|s| s != &self.config.public_origin)
        {
            return Err(Error::forbidden());
        }
        let p = self.authenticate(request).await?;
        let row = self.readable_blob(&p, id).await?;
        let object = self
            .objects
            .bucket
            .get(text(&row, "object_key")?)
            .execute()
            .await
            .map_err(|_| Error::storage())?
            .ok_or_else(Error::storage)?;
        let info = object_info(&object)?;
        if info.sha256 != blob(&row, "sha256")? || info.size != integer(&row, "size_bytes")? as u64
        {
            return Err(Error::storage());
        }
        self.current(&p).await?;
        let mut response = worker::Response::from_body(
            object
                .body()
                .ok_or_else(Error::storage)?
                .response_body()
                .map_err(|_| Error::storage())?,
        )
        .map_err(|_| Error::storage())?;
        for (k, v) in [
            ("content-type", info.content_type.as_str()),
            ("content-disposition", "attachment"),
            ("cache-control", "private, no-store"),
            ("x-content-type-options", "nosniff"),
        ] {
            response
                .headers_mut()
                .set(k, v)
                .map_err(|_| Error::storage())?;
        }
        Ok(response)
    }
}
fn error_response(error: Error) -> worker::Result<worker::Response> {
    worker::Response::from_json(&json!({"ok":false,"error":error.value()}))
        .map(|r| r.with_status(error.status()))
}
fn convert(response: Response) -> worker::Result<worker::Response> {
    let mut r = worker::Response::from_bytes(response.body)?.with_status(response.status);
    for (k, v) in response.headers {
        r.headers_mut().set(&k, &v)?;
    }
    Ok(r)
}
#[event(fetch)]
pub async fn fetch(
    raw: worker::Request,
    env: Env,
    _ctx: Context,
) -> worker::Result<worker::Response> {
    let https = raw.url()?.scheme() == "https";
    let mut response = match handle_fetch(raw, env).await {
        Ok(response) => response,
        Err(_) => error_response(Error::storage())?,
    };
    for (name, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
    ] {
        response.headers_mut().set(name, value)?;
    }
    if https {
        response
            .headers_mut()
            .set("strict-transport-security", "max-age=31536000")?;
    }
    Ok(response)
}

async fn handle_fetch(mut raw: worker::Request, env: Env) -> worker::Result<worker::Response> {
    let production = env
        .var("DEPLOYMENT_ENV")
        .is_ok_and(|value| value.to_string() == "production");
    let config = Config {
        admin_secret: env.secret("WAYPOST_ADMIN_SECRET")?.to_string(),
        public_origin: env.var("PUBLIC_ORIGIN")?.to_string(),
    };
    let origin = worker::Url::parse(&config.public_origin)
        .map_err(|_| worker::Error::from("Invalid public origin"))?;
    if config.admin_secret.len() < 32
        || !matches!(origin.scheme(), "http" | "https")
        || origin.origin().ascii_serialization() != config.public_origin
        || (production && origin.scheme() != "https")
    {
        return error_response(Error::storage());
    }
    if raw
        .headers()
        .get("origin")?
        .is_some_and(|value| value != config.public_origin)
    {
        return error_response(Error::forbidden());
    }
    // Production must not silently omit abuse controls when a binding is missing.
    let admin_limiter = env.rate_limiter("ADMIN_RATE_LIMITER");
    let api_limiter = env.rate_limiter("API_RATE_LIMITER");
    if production && (admin_limiter.is_err() || api_limiter.is_err()) {
        return error_response(Error::storage());
    }
    let path = raw.path();
    let limiter = if path == "/admin/session" && raw.method() == worker::Method::Post {
        admin_limiter.ok()
    } else if path == "/mcp"
        || path == "/ready"
        || path.starts_with("/v0/")
        || path.starts_with("/admin/v0/")
    {
        api_limiter.ok()
    } else {
        None
    };
    if let Some(limiter) = limiter {
        let ip = raw
            .headers()
            .get("cf-connecting-ip")?
            .unwrap_or_else(|| "local".into());
        let outcome = limiter
            .limit(format!("ip:{}", hex::encode(Sha256::digest(ip.as_bytes()))))
            .await?;
        if !outcome.success {
            let mut response = error_response(Error::new(
                "RATE_LIMITED",
                "Request rate limit exceeded; retry later.",
            ))?;
            response.headers_mut().set("retry-after", "60")?;
            return Ok(response);
        }
    }
    let database = env.d1("DB")?;
    let session = database.with_session(Some("first-primary"))?;
    // Conservative Free-tier profile. Public describe reports these exact limits.
    let limits = Limits {
        max_json_bytes: 16384,
        max_blob_bytes: 256 * 1024,
        max_message_attachments: 4,
        max_inline_memory_changes: 8,
        max_manifest_memory_changes: 8,
        max_collection_items: 32,
        max_page_size: 20,
    };
    let app = App::new(
        D1Store { session },
        R2Store {
            bucket: env.bucket("OBJECTS")?,
        },
        config,
    )
    .with_limits(limits);
    let mut request = Request {
        method: raw.method().to_string(),
        path: raw.path(),
        headers: raw.headers().entries().collect(),
        body: vec![],
    };
    if request.path == "/ready" && request.method == "GET" {
        let readiness: Result<Value> = async {
            app.admin_auth(&request)?;
            let migrations = ["0001_identity_mailbox.sql", "0002_receipts.sql", "0003_blobs_memory.sql", "0004_privacy.sql", "0005_admin_sessions.sql", "0006_grant_integrity.sql"];
            let rows = app.db.query(Statement::new("SELECT count(*) AS applied FROM d1_migrations WHERE name IN (?,?,?,?,?,?)", migrations.into_iter().map(Sql::from).collect())).await?;
            if rows.first().map(|row| integer(row, "applied")).transpose()? != Some(6) {
                return Err(Error::storage());
            }
            app.objects.inspect("_waypost_readiness_probe").await?;
            Ok(json!({"status":"ready","schema_revision":6,"environment":if production {"production"} else {"development"}}))
        }.await;
        return match readiness {
            Ok(data) => convert(Response::json(200, json!({"ok":true,"data":data}))),
            Err(error) => error_response(error),
        };
    }
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
        return match app.streaming_upload(&request, &mut raw, parts[2]).await {
            Ok(data) => convert(Response::json(200, json!({"ok":true,"data":data}))),
            Err(e) => error_response(e),
        };
    }
    if request.method == "GET"
        && parts.len() == 4
        && parts[0..2] == ["v0", "blobs"]
        && parts[3] == "content"
    {
        return match app.streaming_download(&request, parts[2]).await {
            Ok(response) => Ok(response),
            Err(e) => error_response(e),
        };
    }
    if raw.inner().body().is_some() {
        let mut stream = raw.stream()?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if request.body.len() + chunk.len() > app.limits.max_json_bytes {
                return error_response(Error::new(
                    "RESOURCE_LIMIT",
                    "JSON exceeds Worker deployment limit.",
                ));
            }
            request.body.extend(chunk);
        }
    }
    convert(app.handle(request).await)
}
