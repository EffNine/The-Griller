//! Output formatting helpers.

use serde_json::Value;

/// Serialize a value as JSON. Compact by default, pretty with `--pretty`.
pub fn to_json(value: &Value, pretty: bool) -> String {
    if pretty {
        serde_json::to_string_pretty(value).expect("serialization to JSON is infallible")
    } else {
        serde_json::to_string(value).expect("serialization to JSON is infallible")
    }
}
