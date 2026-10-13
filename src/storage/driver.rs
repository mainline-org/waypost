use crate::error::{Error, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Text(String),
    Blob(Vec<u8>),
}
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Text(s.into())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Self::Integer(n)
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Integer(i64::from(v))
    }
}
impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Self::Blob(v)
    }
}
impl From<Option<i64>> for Value {
    fn from(v: Option<i64>) -> Self {
        v.map(Self::Integer).unwrap_or(Self::Null)
    }
}
impl From<Option<String>> for Value {
    fn from(v: Option<String>) -> Self {
        v.map(Self::Text).unwrap_or(Self::Null)
    }
}

pub type Row = BTreeMap<String, Value>;
pub fn text(row: &Row, key: &str) -> Result<String> {
    match row.get(key) {
        Some(Value::Text(s)) => Ok(s.clone()),
        _ => Err(Error::internal("decode text column")),
    }
}
pub fn integer(row: &Row, key: &str) -> Result<i64> {
    match row.get(key) {
        Some(Value::Integer(n)) => Ok(*n),
        Some(Value::Text(s)) => s
            .parse()
            .map_err(|_| Error::internal("decode integer column")),
        _ => Err(Error::internal("decode integer column")),
    }
}
pub fn optional_text(row: &Row, key: &str) -> Option<String> {
    match row.get(key) {
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    }
}
pub fn blob(row: &Row, key: &str) -> Result<Vec<u8>> {
    match row.get(key) {
        Some(Value::Blob(b)) => Ok(b.clone()),
        _ => Err(Error::internal("decode blob column")),
    }
}
#[derive(Debug, Clone)]
pub struct Statement {
    pub sql: String,
    pub parameters: Vec<Value>,
}
impl Statement {
    pub fn new(sql: impl Into<String>, parameters: Vec<Value>) -> Self {
        Self {
            sql: sql.into(),
            parameters,
        }
    }
}

// Futures intentionally stay local: Workers streams are not Send.
#[allow(async_fn_in_trait)]
pub trait Database {
    async fn query(&self, statement: Statement) -> Result<Vec<Row>>;
    async fn batch(&self, statements: Vec<Statement>) -> Result<Vec<Vec<Row>>>;
}
