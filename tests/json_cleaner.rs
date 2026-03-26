use fetchless::json_cleaner::clean_json_response;
use serde_json::{json, Value};

#[test]
fn test_removes_junk_keys() {
    let input = json!({
        "data": "important",
        "meta": {"page": 1},
        "tracking": "abc123"
    });
    let result = clean_json_response(&input.to_string());
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.get("data").is_some());
    assert!(parsed.get("meta").is_none());
    assert!(parsed.get("tracking").is_none());
}

#[test]
fn test_recursive_junk_removal() {
    let input = json!({
        "results": [{
            "name": "test",
            "metadata": {"internal": true},
            "value": 42
        }]
    });
    let result = clean_json_response(&input.to_string());
    let parsed: Value = serde_json::from_str(&result).unwrap();
    let first = &parsed["results"][0];
    assert!(first.get("name").is_some());
    assert!(first.get("metadata").is_none());
    assert!(first.get("value").is_some());
}

#[test]
fn test_invalid_json_passthrough() {
    let input = "not json {{{";
    let result = clean_json_response(input);
    assert_eq!(result, input);
}

#[test]
fn test_preserves_non_junk() {
    let input = json!({
        "id": 1,
        "name": "test",
        "items": [1, 2, 3]
    });
    let result = clean_json_response(&input.to_string());
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["id"], 1);
    assert_eq!(parsed["name"], "test");
    assert_eq!(parsed["items"].as_array().unwrap().len(), 3);
}

#[test]
fn test_all_junk_keys_removed() {
    let input = json!({
        "pagination": {},
        "_links": {},
        "debug": {},
        "disclaimer": "text",
        "copyright": "2024",
        "real_data": true
    });
    let result = clean_json_response(&input.to_string());
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.get("real_data").is_some());
    assert!(parsed.get("pagination").is_none());
    assert!(parsed.get("_links").is_none());
    assert!(parsed.get("debug").is_none());
}
