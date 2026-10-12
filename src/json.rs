use crate::error::{Error, Result};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};

struct Unique(Value);
struct UniqueVisitor;
impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = Unique;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("valid JSON with unique object keys")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Unique, E> {
        Ok(Unique(Value::Bool(v)))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
        Ok(Unique(Value::Number(v.into())))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
        Ok(Unique(Value::Number(v.into())))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
        Number::from_f64(v)
            .map(|n| Unique(Value::Number(n)))
            .ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
        Ok(Unique(Value::String(v.into())))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Unique, E> {
        Ok(Unique(Value::String(v)))
    }
    fn visit_none<E: de::Error>(self) -> std::result::Result<Unique, E> {
        Ok(Unique(Value::Null))
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Unique, E> {
        Ok(Unique(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<Unique, A::Error> {
        let mut values = Vec::new();
        while let Some(v) = a.next_element::<Unique>()? {
            values.push(v.0);
        }
        Ok(Unique(Value::Array(values)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<Unique, A::Error> {
        let mut values = Map::new();
        while let Some(k) = a.next_key::<String>()? {
            if values.contains_key(&k) {
                return Err(de::Error::custom("duplicate object key"));
            }
            values.insert(k, a.next_value::<Unique>()?.0);
        }
        Ok(Unique(Value::Object(values)))
    }
}
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        d.deserialize_any(UniqueVisitor)
    }
}
pub fn parse(bytes: &[u8], limit: usize) -> Result<Value> {
    if bytes.len() > limit {
        return Err(Error::new(
            "RESOURCE_LIMIT",
            "Request exceeds the deployment limit.",
        ));
    }
    if bytes.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_slice::<Unique>(bytes)
        .map(|v| v.0)
        .map_err(|_| Error::invalid("Invalid JSON, duplicate keys or unsupported number."))
}
pub fn decode<T: serde::de::DeserializeOwned>(v: Value) -> Result<T> {
    serde_json::from_value(v).map_err(|_| Error::invalid("Request does not match the schema."))
}
pub fn fields(v: &Value, allowed: &[&str]) -> Result<()> {
    let object = v
        .as_object()
        .ok_or_else(|| Error::invalid("Expected an object."))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Error::invalid("Unknown request field."));
    }
    Ok(())
}
pub fn required(v: &Value, key: &str) -> Result<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::invalid(format!("Missing string field: {key}")))
}
pub fn canonical(v: &Value) -> Result<Vec<u8>> {
    serde_jcs::to_vec(v).map_err(|_| Error::invalid("Content cannot be canonicalized."))
}
