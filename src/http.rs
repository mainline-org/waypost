use serde_json::Value;
use std::collections::BTreeMap;

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
            headers: BTreeMap::from([("content-type".into(), "application/json".into())]),
            body: serde_json::to_vec(&value).expect("JSON value serializes"),
        }
    }
}
