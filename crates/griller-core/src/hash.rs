//! Canonical (deterministic) serialization and BLAKE3 content hashing.
//!
//! ## Semantics
//!
//! * `canonical_bytes` serializes a value to JSON with every object's keys
//!   sorted recursively. The same logical value always produces the same
//!   byte sequence, regardless of insertion order or struct field order.
//! * `content_hash` is `BLAKE3(canonical_bytes(value))`, hex encoded.
//! * The hash is computed over explicitly chosen payloads (see
//!   [`crate::evidence`]). Unstable fields such as timestamps and process
//!   durations are excluded from hashing unless the caller includes them on
//!   purpose.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::CoreError;

/// Recursively sort the object keys of a JSON value.
fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                let inner = map.get(&key).cloned().unwrap_or(Value::Null);
                sorted.insert(key, sort_value(inner));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

/// Convert any serializable value into a canonical JSON [`Value`] with sorted
/// object keys, recursively.
pub fn canonical_value<T: Serialize>(value: &T) -> Result<Value, CoreError> {
    let value = serde_json::to_value(value)?;
    Ok(sort_value(value))
}

/// Serialize a value to deterministic UTF-8 JSON bytes with sorted keys.
pub fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CoreError> {
    Ok(serde_json::to_vec(&canonical_value(value)?)?)
}

/// Serialize a value to a deterministic JSON string with sorted keys.
pub fn canonical_string<T: Serialize>(value: &T) -> Result<String, CoreError> {
    Ok(serde_json::to_string(&canonical_value(value)?)?)
}

/// Compute the BLAKE3 hash of raw bytes, hex encoded (lowercase).
pub fn blake3_hex(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// Compute `BLAKE3(canonical_bytes(value))`, hex encoded.
///
/// This is the single function used to derive evidence content hashes.
pub fn content_hash<T: Serialize>(value: &T) -> Result<String, CoreError> {
    Ok(blake3_hex(&canonical_bytes(value)?))
}

/// Compute the BLAKE3 hash of a string value, hex encoded.
pub fn hash_string(data: &str) -> String {
    blake3_hex(data.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonicalization_sorts_keys() {
        let a = json!({ "b": 1, "a": [ { "z": true, "y": null } ], "c": "x" });
        let b = json!({ "c": "x", "a": [ { "y": null, "z": true } ], "b": 1 });
        assert_eq!(canonical_bytes(&a).unwrap(), canonical_bytes(&b).unwrap());
        assert_eq!(canonical_string(&a).unwrap(), canonical_string(&b).unwrap());
    }

    #[test]
    fn canonical_string_is_stable() {
        let value = json!({ "x": { "n": [1, 2, 3], "s": "hello" } });
        let first = canonical_string(&value).unwrap();
        let second = canonical_string(&value).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn content_hash_differs_on_content() {
        let a = json!({ "v": "one" });
        let b = json!({ "v": "two" });
        assert_ne!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
        assert_eq!(content_hash(&a).unwrap(), content_hash(&a).unwrap());
    }

    #[test]
    fn blake3_hex_is_64_chars() {
        let h = blake3_hex(b"the griller");
        assert_eq!(h.len(), 64);
    }
}
