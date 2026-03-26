use fetchless::db::{cache_get, cache_set, get_stats, init_db, log_fetch, log_prompt, Db};

fn test_db() -> Db {
    init_db(":memory:").unwrap()
}

#[test]
fn test_cache_set_and_get() {
    let db = test_db();
    cache_set(&db, "https://example.com", "content", 100, 10, 300);
    let entry = cache_get(&db, "https://example.com").unwrap();
    assert_eq!(entry.payload, "content");
    assert_eq!(entry.original_tokens, 100);
    assert_eq!(entry.cleaned_tokens, 10);
}

#[test]
fn test_cache_miss() {
    let db = test_db();
    assert!(cache_get(&db, "https://nonexistent.com").is_none());
}

#[test]
fn test_cache_expired() {
    let db = test_db();
    // TTL of 0 → immediately expired
    cache_set(&db, "https://expired.com", "old", 50, 5, 0);
    assert!(cache_get(&db, "https://expired.com").is_none());
}

#[test]
fn test_log_fetch_and_stats() {
    let db = test_db();
    log_fetch(&db, "https://test.com", 1000, 100, false, "");
    log_fetch(&db, "https://test2.com", 500, 50, true, "");
    let stats = get_stats(&db);
    assert_eq!(stats.layer2_fetch_requests, 2);
    assert_eq!(stats.layer2_tokens_saved, 1350);
    assert_eq!(stats.layer2_cache_hits, 1);
}

#[test]
fn test_log_prompt_and_stats() {
    let db = test_db();
    log_prompt(&db, "abc", 200, 150, 0.95, true, "");
    let stats = get_stats(&db);
    assert_eq!(stats.layer1_refine_requests, 1);
    assert_eq!(stats.layer1_tokens_saved, 50);
}

#[test]
fn test_stats_empty_db() {
    let db = test_db();
    let stats = get_stats(&db);
    assert_eq!(stats.total_tokens_saved, 0);
    assert_eq!(stats.est_cost_saved, 0.0);
}
