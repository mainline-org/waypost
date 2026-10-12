use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const MAX_REVISION: u64 = 9_007_199_254_740_991;
pub fn valid_id(s: &str) -> bool {
    (1..=96).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
pub fn check_id(s: &str) -> Result<()> {
    if valid_id(s) {
        Ok(())
    } else {
        Err(Error::invalid("Invalid resource ID."))
    }
}
pub fn check_request_id(s: &str) -> Result<()> {
    if (1..=128).contains(&s.len())
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.:".contains(&c))
    {
        Ok(())
    } else {
        Err(Error::invalid("Invalid request ID."))
    }
}
macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: String) -> Result<Self> {
                check_id(&value)?;
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
identifier!(SpaceId);
identifier!(ActorId);
identifier!(ConsumerId);
identifier!(MessageId);
identifier!(CollectionId);
identifier!(BlobId);
identifier!(ThreadId);
identifier!(FeedItemId);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize)]
pub struct Revision(u64);
impl Revision {
    pub fn new(value: u64) -> Result<Self> {
        if value <= MAX_REVISION {
            Ok(Self(value))
        } else {
            Err(Error::new("RESOURCE_LIMIT", "Revision limit reached."))
        }
    }
    pub fn value(self) -> u64 {
        self.0
    }
}
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct VisibleSeq(u64);
impl VisibleSeq {
    pub fn new(value: u64) -> Result<Self> {
        if value > 0 && value <= i64::MAX as u64 {
            Ok(Self(value))
        } else {
            Err(Error::invalid("Invalid sequence."))
        }
    }
    pub fn value(self) -> u64 {
        self.0
    }
}
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Info,
    Question,
    Result,
    TodoSuggestion,
    MemoryNotice,
    ProcessingResult,
}
impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Question => "question",
            Self::Result => "result",
            Self::TodoSuggestion => "todo_suggestion",
            Self::MemoryNotice => "memory_notice",
            Self::ProcessingResult => "processing_result",
        }
    }
}
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStage {
    Received,
    Processed,
}
impl ReceiptStage {
    pub fn rank(self) -> i64 {
        match self {
            Self::Received => 1,
            Self::Processed => 2,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Processed => "processed",
        }
    }
}
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    Snapshot,
    Delta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageBody {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Send {
    pub request_id: String,
    #[serde(default)]
    pub recipient_ids: Option<Vec<String>>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub in_reply_to: Option<String>,
    pub kind: MessageKind,
    pub subject: String,
    pub body: MessageBody,
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}
impl Send {
    pub fn validate(&self) -> Result<()> {
        check_request_id(&self.request_id)?;
        if self.subject.is_empty() || self.subject.len() > 256 || self.body.options.len() > 20 {
            return Err(Error::invalid("Invalid subject or message options."));
        }
        if self.attachment_ids.len() > 16 {
            return Err(Error::new("RESOURCE_LIMIT", "Too many attachments."));
        }
        for id in &self.attachment_ids {
            check_id(id)?;
        }
        if let Some(id) = &self.thread_id {
            check_id(id)?;
        }
        if let Some(id) = &self.in_reply_to {
            check_id(id)?;
            if self.recipient_ids.is_none() {
                return Err(Error::invalid("Replies require explicit recipients."));
            }
        }
        if let Some(ids) = &self.recipient_ids {
            if ids.is_empty() || ids.len() > 16 {
                return Err(Error::invalid("Invalid recipient count."));
            }
            let mut unique = std::collections::BTreeSet::new();
            for id in ids {
                check_id(id)?;
                if !unique.insert(id) {
                    return Err(Error::invalid("Duplicate recipient."));
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FeedFilter {
    #[serde(default)]
    pub producer_ids: Vec<String>,
    #[serde(default)]
    pub thread_ids: Vec<String>,
    #[serde(default)]
    pub item_types: Vec<String>,
    #[serde(default)]
    pub collection_ids: Vec<String>,
}
impl FeedFilter {
    pub fn normalize(&mut self) -> Result<()> {
        let mut total = 0;
        for values in [
            &mut self.producer_ids,
            &mut self.thread_ids,
            &mut self.collection_ids,
        ] {
            for value in values.iter() {
                check_id(value)?;
            }
            values.sort();
            values.dedup();
            total += values.len();
        }
        for t in &self.item_types {
            if !["message", "memory_revision", "receipt"].contains(&t.as_str()) {
                return Err(Error::invalid("Unknown feed type."));
            }
        }
        self.item_types.sort();
        self.item_types.dedup();
        total += self.item_types.len();
        if total > 64 {
            return Err(Error::new("RESOURCE_LIMIT", "Too many filter values."));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ResourceGrants {
    #[serde(default)]
    pub recipient_ids: Vec<String>,
    #[serde(default)]
    pub memory_read_collection_ids: Vec<String>,
    #[serde(default)]
    pub memory_write_collection_ids: Vec<String>,
    #[serde(default)]
    pub blob_reference_ids: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct Principal {
    pub space_id: String,
    pub actor_id: String,
    pub consumer_id: String,
    pub token_id: String,
    pub actor_version: i64,
    pub credential_version: i64,
    pub view_revision: i64,
    pub default_actor_id: String,
    pub view: String,
    pub scopes: Vec<String>,
    pub grants: ResourceGrants,
    pub filter: FeedFilter,
    pub grant_fingerprint: String,
    pub filter_fingerprint: String,
}
impl Principal {
    pub fn has(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
    pub fn require(&self, scope: &str) -> Result<()> {
        if self.has(scope) {
            Ok(())
        } else {
            Err(Error::forbidden())
        }
    }
}
