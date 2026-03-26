use serde_json::Value;

/// Keys considered junk in JSON API responses
const JUNK_KEYS: &[&str] = &[
    "meta", "metadata", "tracking", "ads", "advertisement",
    "pagination", "paging", "links", "_links", "debug",
    "request_id", "trace_id", "server", "timing",
    "disclaimer", "copyright", "legal",
];

/// Remove common junk keys from JSON API responses.
pub fn clean_json_response(raw_json: &str) -> String {
    let data: Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(_) => return raw_json.to_string(),
    };

    let cleaned = strip_junk(data);
    serde_json::to_string_pretty(&cleaned).unwrap_or_else(|_| raw_json.to_string())
}

fn strip_junk(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let cleaned: serde_json::Map<String, Value> = map
                .into_iter()
                .filter(|(k, _)| !JUNK_KEYS.contains(&k.to_lowercase().as_str()))
                .map(|(k, v)| (k, strip_junk(v)))
                .collect();
            Value::Object(cleaned)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(strip_junk).collect()),
        other => other,
    }
}
