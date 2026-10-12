use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
    pub details: Value,
}
impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: json!({}),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENT", message)
    }
    pub fn storage() -> Self {
        Self::new("STORAGE_UNAVAILABLE", "Storage is temporarily unavailable.")
    }
    pub fn not_found() -> Self {
        Self::new("NOT_FOUND", "Resource was not found.")
    }
    pub fn unauthenticated() -> Self {
        Self::new("UNAUTHENTICATED", "A valid credential is required.")
    }
    pub fn forbidden() -> Self {
        Self::new("FORBIDDEN", "The operation is not permitted.")
    }
    pub fn status(&self) -> u16 {
        match self.code {
            "UNAUTHENTICATED" => 401,
            "FORBIDDEN" => 403,
            "NOT_FOUND" => 404,
            "IDEMPOTENCY_CONFLICT"
            | "REVISION_CONFLICT"
            | "CURSOR_SCOPE_CHANGED"
            | "BLOB_NOT_SEALED"
            | "UPLOAD_SEALED"
            | "UPLOAD_CONTENT_CONFLICT"
            | "SUBMISSION_PENDING" => 409,
            "CURSOR_EXPIRED" | "UPLOAD_EXPIRED" => 410,
            "RESOURCE_LIMIT" => 413,
            "CHECKSUM_MISMATCH" | "INVALID_BLOB" => 422,
            "RATE_LIMITED" => 429,
            "STORAGE_UNAVAILABLE" => 503,
            _ => 400,
        }
    }
    pub fn value(&self) -> Value {
        json!({"code":self.code,"message":self.message,
            "retryable":matches!(self.code,"STORAGE_UNAVAILABLE"|"RATE_LIMITED"|"SUBMISSION_PENDING"),
            "details":self.details})
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
