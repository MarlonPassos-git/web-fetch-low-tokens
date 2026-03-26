use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub type Db = Arc<Mutex<Connection>>;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub fn init_db(path: &str) -> Result<Db, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS prompt_log (
            request_id TEXT PRIMARY KEY,
            timestamp INTEGER,
            original_tokens INTEGER,
            optimized_tokens INTEGER,
            confidence REAL,
            sent_optimized INTEGER,
            skip_reason TEXT,
            savings_pct REAL
        );
        CREATE TABLE IF NOT EXISTS data_cache (
            cache_key TEXT PRIMARY KEY,
            url TEXT,
            payload TEXT,
            original_tokens INTEGER,
            cleaned_tokens INTEGER,
            fetched_at INTEGER,
            ttl_sec INTEGER
        );
        CREATE TABLE IF NOT EXISTS data_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER,
            url TEXT,
            original_tokens INTEGER,
            cleaned_tokens INTEGER,
            from_cache INTEGER,
            reduction_pct REAL,
            error TEXT
        );
        ",
    )?;
    Ok(Arc::new(Mutex::new(conn)))
}

fn cache_key(url: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(url.as_bytes());
    format!("{:x}", hash)[..16].to_string()
}

pub struct CacheEntry {
    pub payload: String,
    pub original_tokens: i64,
    pub cleaned_tokens: i64,
}

pub fn cache_get(db: &Db, url: &str) -> Option<CacheEntry> {
    let conn = db.lock().unwrap();
    let key = cache_key(url);
    let now = now_secs();
    conn.query_row(
        "SELECT payload, original_tokens, cleaned_tokens, fetched_at, ttl_sec \
         FROM data_cache WHERE cache_key = ?1",
        params![key],
        |row| {
            let fetched_at: i64 = row.get(3)?;
            let ttl: i64 = row.get(4)?;
            if now - fetched_at < ttl {
                Ok(Some(CacheEntry {
                    payload: row.get(0)?,
                    original_tokens: row.get(1)?,
                    cleaned_tokens: row.get(2)?,
                }))
            } else {
                Ok(None)
            }
        },
    )
    .unwrap_or(None)
}

pub fn cache_set(
    db: &Db,
    url: &str,
    payload: &str,
    original_tokens: usize,
    cleaned_tokens: usize,
    ttl: u64,
) {
    let conn = db.lock().unwrap();
    let key = cache_key(url);
    let _ = conn.execute(
        "INSERT OR REPLACE INTO data_cache VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            key,
            url,
            payload,
            original_tokens as i64,
            cleaned_tokens as i64,
            now_secs(),
            ttl as i64
        ],
    );
}

pub fn log_fetch(
    db: &Db,
    url: &str,
    original_tokens: usize,
    cleaned_tokens: usize,
    from_cache: bool,
    error: &str,
) {
    let reduction = if original_tokens > 0 {
        (1.0 - cleaned_tokens as f64 / original_tokens as f64) * 100.0
    } else {
        0.0
    };
    let conn = db.lock().unwrap();
    let _ = conn.execute(
        "INSERT INTO data_log (timestamp, url, original_tokens, cleaned_tokens, from_cache, reduction_pct, error) \
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            now_secs(),
            url,
            original_tokens as i64,
            cleaned_tokens as i64,
            from_cache as i64,
            reduction,
            error
        ],
    );
}

pub fn log_prompt(
    db: &Db,
    request_id: &str,
    original_tokens: usize,
    optimized_tokens: usize,
    confidence: f64,
    sent_optimized: bool,
    skip_reason: &str,
) {
    let savings = if original_tokens > 0 {
        (1.0 - optimized_tokens as f64 / original_tokens as f64) * 100.0
    } else {
        0.0
    };
    let conn = db.lock().unwrap();
    let _ = conn.execute(
        "INSERT INTO prompt_log VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            request_id,
            now_secs(),
            original_tokens as i64,
            optimized_tokens as i64,
            confidence,
            sent_optimized as i64,
            skip_reason,
            savings
        ],
    );
}

#[derive(Debug, serde::Serialize)]
pub struct Stats {
    pub layer1_refine_requests: i64,
    pub layer1_tokens_saved: i64,
    pub layer2_fetch_requests: i64,
    pub layer2_tokens_saved: i64,
    pub layer2_cache_hits: i64,
    pub total_tokens_saved: i64,
    pub est_cost_saved: f64,
}

pub fn get_stats(db: &Db) -> Stats {
    let conn = db.lock().unwrap();

    let (prompt_count, prompt_orig, prompt_opt): (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(original_tokens),0), COALESCE(SUM(optimized_tokens),0) FROM prompt_log",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap_or((0, 0, 0));

    let (data_count, data_orig, data_clean, cache_hits): (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(original_tokens),0), COALESCE(SUM(cleaned_tokens),0), COALESCE(SUM(from_cache),0) FROM data_log",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap_or((0, 0, 0, 0));

    let prompt_saved = prompt_orig - prompt_opt;
    let data_saved = data_orig - data_clean;
    let total_saved = prompt_saved + data_saved;

    Stats {
        layer1_refine_requests: prompt_count,
        layer1_tokens_saved: prompt_saved,
        layer2_fetch_requests: data_count,
        layer2_tokens_saved: data_saved,
        layer2_cache_hits: cache_hits,
        total_tokens_saved: total_saved,
        est_cost_saved: total_saved as f64 * 0.000015,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Insert with TTL of 0 so it's immediately expired
        {
            let conn = db.lock().unwrap();
            let key = cache_key("https://expired.com");
            conn.execute(
                "INSERT INTO data_cache VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![key, "https://expired.com", "old", 50i64, 5i64, 0i64, 1i64],
            )
            .unwrap();
        }
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
}
