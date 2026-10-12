use super::{
    ObjectInfo,
    driver::{Row, blob, integer, optional_text, text},
};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionState {
    Reserved,
    Prepared,
    Committed,
    Failed,
}
#[derive(Debug)]
pub struct Submission {
    pub id: String,
    pub operation: String,
    pub resource_id: String,
    pub fingerprint: Vec<u8>,
    pub state: SubmissionState,
}
impl Submission {
    pub fn decode(row: &Row) -> Result<Self> {
        let state = match text(row, "state")?.as_str() {
            "reserved" => SubmissionState::Reserved,
            "prepared" => SubmissionState::Prepared,
            "committed" => SubmissionState::Committed,
            "failed" => SubmissionState::Failed,
            _ => return Err(Error::internal("decode submission state")),
        };
        let fingerprint = blob(row, "fingerprint")?;
        if fingerprint.len() != 32 {
            return Err(Error::internal("decode submission fingerprint"));
        }
        Ok(Self {
            id: text(row, "submission_id")?,
            operation: text(row, "operation")?,
            resource_id: text(row, "resource_id")?,
            fingerprint,
            state,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadState {
    Pending,
    Sealed,
}
pub struct Upload {
    pub state: UploadState,
    pub expires_at: i64,
    pub object_key: String,
    pub info: ObjectInfo,
}
impl Upload {
    pub fn decode(row: &Row) -> Result<Self> {
        let state = match text(row, "state")?.as_str() {
            "pending" => UploadState::Pending,
            "sealed" => UploadState::Sealed,
            _ => return Err(Error::internal("decode upload state")),
        };
        Ok(Self {
            state,
            expires_at: integer(row, "expires_at")?,
            object_key: text(row, "object_key")?,
            info: blob_info(row)?,
        })
    }
    pub fn writable(&self, now: i64) -> Result<()> {
        if self.state == UploadState::Sealed {
            return Err(Error::new("UPLOAD_SEALED", "Upload is sealed."));
        }
        if self.expires_at <= now {
            return Err(Error::new("UPLOAD_EXPIRED", "Upload expired."));
        }
        Ok(())
    }
}
pub fn blob_info(row: &Row) -> Result<ObjectInfo> {
    let size = u64::try_from(integer(row, "size_bytes")?)
        .map_err(|_| Error::internal("decode object size"))?;
    let sha256 = blob(row, "sha256")?;
    if sha256.len() != 32 {
        return Err(Error::internal("decode object checksum"));
    }
    Ok(ObjectInfo {
        size,
        sha256,
        content_type: text(row, "content_type")?,
    })
}
pub fn verify_object(row: &Row, actual: &ObjectInfo) -> Result<()> {
    let expected = blob_info(row)?;
    if expected.size != actual.size
        || expected.sha256 != actual.sha256
        || expected.content_type != actual.content_type
    {
        return Err(Error::internal("stored object integrity"));
    }
    Ok(())
}
pub enum FeedKind {
    Message,
    MemoryRevision,
    Receipt,
}
pub struct FeedItem {
    pub id: String,
    pub resource_id: String,
    pub producer_id: String,
    pub thread_id: Option<String>,
    pub version: i64,
    pub committed_at: i64,
    pub position: u64,
    pub kind: FeedKind,
}
impl FeedItem {
    pub fn decode(row: &Row) -> Result<Self> {
        let kind = match text(row, "item_type")?.as_str() {
            "message" => FeedKind::Message,
            "memory_revision" => FeedKind::MemoryRevision,
            "receipt" => FeedKind::Receipt,
            _ => return Err(Error::internal("decode feed kind")),
        };
        let position = u64::try_from(integer(row, "visible_seq")?)
            .map_err(|_| Error::internal("decode feed position"))?;
        Ok(Self {
            id: text(row, "item_id")?,
            resource_id: text(row, "resource_id")?,
            producer_id: text(row, "producer_id")?,
            thread_id: optional_text(row, "thread_id"),
            version: integer(row, "resource_version")?,
            committed_at: integer(row, "committed_at")?,
            position,
            kind,
        })
    }
}
