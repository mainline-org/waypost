use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route<'a> {
    PublicAsset,
    Health,
    Ready,
    AdminSession,
    Admin,
    Mcp,
    Describe,
    UploadPrepare,
    UploadContent(&'a str),
    UploadCommit(&'a str),
    BlobMetadata(&'a str),
    BlobContent(&'a str),
    MessageSend,
    MessageGet(&'a str),
    Receipts,
    Inbox,
    SubmissionStatus,
    MemoryPublish(&'a str),
    MemoryRead(&'a str),
    Unknown,
}
pub(crate) fn classify<'a>(method: &str, path: &'a str) -> Route<'a> {
    use Route::*;
    match (method, path) {
        (
            "GET",
            "/"
            | "/dashboard"
            | "/dashboard/"
            | "/dashboard/app.js"
            | "/dashboard/style.css"
            | "/dashboard/memory.js"
            | "/dashboard/onboarding-copy.js",
        ) => return PublicAsset,
        ("GET", "/health") => return Health,
        ("GET", "/ready") => return Ready,
        (_, "/admin/session" | "/admin/invitation" | "/admin/access-request") => {
            return AdminSession;
        }
        (_, "/mcp") => return Mcp,
        ("GET", "/v0/describe") => return Describe,
        ("POST", "/v0/uploads") => return UploadPrepare,
        ("POST", "/v0/messages") => return MessageSend,
        ("POST", "/v0/receipts") => return Receipts,
        ("POST", "/v0/inbox/read") => return Inbox,
        ("POST", "/v0/submissions/status") => return SubmissionStatus,
        (_, path) if path.starts_with("/admin/v0/") => return Admin,
        _ => {}
    }
    let parts = path.split('/').collect::<Vec<_>>();
    match (method, parts.as_slice()) {
        ("PUT", ["", "v0", "uploads", id, "content"]) if !id.is_empty() => UploadContent(id),
        ("POST", ["", "v0", "uploads", id, "commit"]) if !id.is_empty() => UploadCommit(id),
        ("GET", ["", "v0", "blobs", id, "content"]) if !id.is_empty() => BlobContent(id),
        ("GET", ["", "v0", "blobs", id]) if !id.is_empty() => BlobMetadata(id),
        ("GET", ["", "v0", "messages", id]) if !id.is_empty() => MessageGet(id),
        ("POST", ["", "v0", "memory", "collections", id, "revisions"]) if !id.is_empty() => {
            MemoryPublish(id)
        }
        ("POST", ["", "v0", "memory", "collections", id, "read"]) if !id.is_empty() => {
            MemoryRead(id)
        }
        _ => Unknown,
    }
}

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn json(method: &str, path: &str, token: &str, value: Value) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            headers: BTreeMap::from([
                ("authorization".into(), format!("Bearer {token}")),
                ("content-type".into(), "application/json".into()),
            ]),
            body: serde_json::to_vec(&value).expect("JSON value serializes"),
        }
    }
}

pub struct Response {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            headers: BTreeMap::from([
                ("content-type".into(), "application/json".into()),
                ("cache-control".into(), "no-store".into()),
                ("x-content-type-options".into(), "nosniff".into()),
            ]),
            body: serde_json::to_vec(&value).expect("JSON value serializes"),
        }
    }
}
