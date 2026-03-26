/// E2E tests for the Fetchless HTTP server.
///
/// Offline tests (default): run without internet access.
/// Online tests (ignored): require internet, run with `cargo test -- --include-ignored`.
use axum_test::TestServer;
use fetchless::config::Config;
use fetchless::server::{AppState, build_router};
use fetchless::{db, html_cleaner};
use serde_json::{Value, json};

// ─── helpers ────────────────────────────────────────────────────────────────

fn make_server() -> TestServer {
    let database = db::init_db(":memory:").expect("in-memory DB");
    let client = reqwest::Client::new();
    let config = Config {
        port: 8080,
        bind: "127.0.0.1".to_string(),
        db_path: ":memory:".to_string(),
        default_ttl: 300,
        mcp: false,
    };
    let state = AppState { db: database, client, config };
    TestServer::new(build_router(state))
}

// ─── GET / ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_home_returns_server_info() {
    let server = make_server();
    let res = server.get("/").await;
    res.assert_status_ok();

    let body: Value = res.json();
    assert_eq!(body["name"], "Fetchless");
    assert!(body["version"].is_string());
    assert!(body["endpoints"].is_object());
    assert!(body["stats"].is_object());
}

// ─── GET /stats ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_stats_returns_valid_structure() {
    let server = make_server();
    let res = server.get("/stats").await;
    res.assert_status_ok();

    let body: Value = res.json();
    assert!(body["layer1_refine_requests"].is_number(), "missing layer1_refine_requests");
    assert!(body["layer1_tokens_saved"].is_number(), "missing layer1_tokens_saved");
    assert!(body["layer2_fetch_requests"].is_number(), "missing layer2_fetch_requests");
    assert!(body["layer2_tokens_saved"].is_number(), "missing layer2_tokens_saved");
    assert!(body["total_tokens_saved"].is_number(), "missing total_tokens_saved");
    assert!(body["est_cost_saved"].is_number(), "missing est_cost_saved");
}

#[tokio::test]
async fn test_stats_accumulates_after_refine() {
    let server = make_server();

    // Before
    let before: Value = server.get("/stats").await.json();
    let before_count = before["layer1_refine_requests"].as_i64().unwrap();

    // Refine something
    server.post("/refine")
        .json(&json!({"text": "Basically just wanted to kind of maybe ask you about something."}))
        .await
        .assert_status_ok();

    // After
    let after: Value = server.get("/stats").await.json();
    let after_count = after["layer1_refine_requests"].as_i64().unwrap();
    assert_eq!(after_count, before_count + 1, "refine request should be counted in stats");
}

// ─── POST /refine ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_refine_returns_suggestion() {
    let server = make_server();
    let res = server.post("/refine")
        .json(&json!({"text": "Basically I just wanted to maybe kind of ask you something."}))
        .await;
    res.assert_status_ok();

    let body: Value = res.json();
    assert!(body["original"].is_string());
    assert!(body["suggested"].is_string());
    assert!(body["savings_pct"].is_number());
    assert!(body["confidence"].is_number());
    assert!(body["protected_entities"].is_array());
    assert!(!body["original"].as_str().unwrap().is_empty());
    assert!(!body["suggested"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_refine_accepts_prompt_field_alias() {
    let server = make_server();
    let res = server.post("/refine")
        .json(&json!({"prompt": "Just basically wanted to ask about something."}))
        .await;
    res.assert_status_ok();

    let body: Value = res.json();
    assert!(body["suggested"].is_string());
}

#[tokio::test]
async fn test_refine_empty_text_returns_error() {
    let server = make_server();
    let res = server.post("/refine")
        .json(&json!({"text": ""}))
        .await;
    // Should return 4xx
    assert!(
        res.status_code().as_u16() >= 400,
        "empty text should return an error, got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn test_refine_missing_field_returns_error() {
    let server = make_server();
    let res = server.post("/refine")
        .json(&json!({"wrong_key": "hello"}))
        .await;
    assert!(
        res.status_code().as_u16() >= 400,
        "missing text field should return an error"
    );
}

#[tokio::test]
async fn test_refine_savings_are_non_negative() {
    let server = make_server();
    let res = server.post("/refine")
        .json(&json!({"text": "Hello world."}))
        .await;
    res.assert_status_ok();

    let body: Value = res.json();
    let savings = body["savings_pct"].as_f64().unwrap();
    assert!(savings >= 0.0, "savings_pct should never be negative, got {savings}");
}

// ─── POST /fetch — error cases (offline) ─────────────────────────────────────

#[tokio::test]
async fn test_fetch_rejects_http_scheme() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": "http://example.com/page"}))
        .await;
    assert!(
        res.status_code().as_u16() >= 400,
        "HTTP URL should be rejected (SSRF), got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn test_fetch_rejects_private_ip() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": "https://192.168.1.1/secret"}))
        .await;
    assert!(
        res.status_code().as_u16() >= 400,
        "Private IP should be rejected (SSRF)"
    );
}

#[tokio::test]
async fn test_fetch_rejects_loopback() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": "https://127.0.0.1/admin"}))
        .await;
    assert!(
        res.status_code().as_u16() >= 400,
        "Loopback IP should be rejected (SSRF)"
    );
}

#[tokio::test]
async fn test_fetch_rejects_empty_url() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": ""}))
        .await;
    assert!(res.status_code().as_u16() >= 400, "Empty URL should return an error");
}

#[tokio::test]
async fn test_fetch_rejects_missing_url_field() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"wrong": "field"}))
        .await;
    assert!(res.status_code().as_u16() >= 400, "Missing url field should return error");
}

#[tokio::test]
async fn test_fetch_rejects_malformed_url() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": "not-a-url-at-all"}))
        .await;
    assert!(res.status_code().as_u16() >= 400, "Malformed URL should return error");
}

// ─── POST /fetch/batch — error cases (offline) ───────────────────────────────

#[tokio::test]
async fn test_batch_rejects_more_than_10_urls() {
    let server = make_server();
    let urls: Vec<String> = (1..=11)
        .map(|i| format!("https://example{i}.com/"))
        .collect();
    let res = server.post("/fetch/batch")
        .json(&json!({"urls": urls}))
        .await;
    assert!(
        res.status_code().as_u16() >= 400,
        "Batch of 11 URLs should be rejected"
    );
}

#[tokio::test]
async fn test_batch_rejects_empty_urls_array() {
    let server = make_server();
    let res = server.post("/fetch/batch")
        .json(&json!({"urls": []}))
        .await;
    assert!(res.status_code().as_u16() >= 400, "Empty urls array should be rejected");
}

#[tokio::test]
async fn test_batch_rejects_private_ip_in_batch() {
    let server = make_server();
    let res = server.post("/fetch/batch")
        .json(&json!({"urls": ["https://example.com/", "https://10.0.0.1/internal"]}))
        .await;
    assert!(
        res.status_code().as_u16() >= 400,
        "Batch with private IP should be rejected"
    );
}

// ─── html_cleaner: Wikipedia HTML structure ───────────────────────────────────
//
// These unit tests reproduce the structure Wikipedia actually serves so we can
// catch regressions in the HTML cleaning pipeline without network access.

fn wikipedia_like_html() -> &'static str {
    r#"<!DOCTYPE html>
<html>
<head><title>Rust (programming language) - Wikipedia</title></head>
<body class="mediawiki ltr sitedir-ltr">
  <header id="mw-header" class="vector-header">
    <nav class="vector-main-menu-container">
      <div class="vector-main-menu">Navigation links here</div>
    </nav>
  </header>
  <div id="mw-page-base" class="noprint"></div>
  <div id="mw-navigation">
    <div id="mw-panel">
      <div class="portal">
        <ul><li>Main page</li><li>Contents</li></ul>
      </div>
    </div>
  </div>
  <div id="content" class="mw-body" role="main">
    <h1 id="firstHeading" class="firstHeading">Rust (programming language)</h1>
    <div id="bodyContent" class="vector-body">
      <div id="siteSub" class="noprint">From Wikipedia, the free encyclopedia</div>
      <div id="mw-content-text" class="mw-body-content">
        <div class="mw-parser-output">
          <p>Rust is a multi-paradigm, general-purpose programming language that emphasizes performance, type safety, and concurrency.</p>
          <p>It enforces memory safety, meaning that all references point to valid memory.</p>
          <h2><span class="mw-headline">History</span></h2>
          <p>Rust was originally designed by Graydon Hoare at Mozilla Research in 2010.</p>
          <h2><span class="mw-headline">Features</span></h2>
          <p>Rust features include ownership, borrowing, and lifetimes.</p>
          <div class="navbox">
            <table><tr><td>Navigation box — should be filtered</td></tr></table>
          </div>
          <div class="mw-references-wrap">
            <ol class="references">
              <li>Reference 1</li>
              <li>Reference 2</li>
            </ol>
          </div>
        </div>
      </div>
    </div>
  </div>
  <footer id="footer" role="contentinfo">
    <div id="footer-info">Footer content here</div>
  </footer>
</body>
</html>"#
}

#[test]
fn test_html_cleaner_wikipedia_article_has_content() {
    let result = html_cleaner::clean_html(wikipedia_like_html());
    assert!(
        !result.trim().is_empty(),
        "Wikipedia-like HTML should produce non-empty content after cleaning"
    );
}

#[test]
fn test_html_cleaner_wikipedia_preserves_article_text() {
    let result = html_cleaner::clean_html(wikipedia_like_html());
    assert!(
        result.contains("multi-paradigm"),
        "Main article paragraph should be preserved.\nGot: {result}"
    );
    assert!(
        result.contains("memory safety"),
        "Second paragraph should be preserved.\nGot: {result}"
    );
    assert!(
        result.contains("Graydon Hoare"),
        "History section text should be preserved.\nGot: {result}"
    );
}

#[test]
fn test_html_cleaner_wikipedia_removes_navbox() {
    let result = html_cleaner::clean_html(wikipedia_like_html());
    assert!(
        !result.contains("Navigation box — should be filtered"),
        "navbox element should be filtered out.\nGot: {result}"
    );
}

#[test]
fn test_html_cleaner_wikipedia_removes_footer() {
    let result = html_cleaner::clean_html(wikipedia_like_html());
    assert!(
        !result.contains("Footer content here"),
        "Footer should be filtered out.\nGot: {result}"
    );
}

#[test]
fn test_html_cleaner_wikipedia_removes_navigation() {
    let result = html_cleaner::clean_html(wikipedia_like_html());
    assert!(
        !result.contains("Navigation links here"),
        "Navigation links should be filtered out.\nGot: {result}"
    );
}

#[test]
fn test_html_cleaner_wikipedia_has_meaningful_reduction() {
    let raw = wikipedia_like_html();
    let cleaned = html_cleaner::clean_html(raw);
    let original_len = raw.len();
    let cleaned_len = cleaned.len();
    assert!(
        cleaned_len < original_len / 2,
        "Cleaned content ({cleaned_len} chars) should be significantly smaller than raw HTML ({original_len} chars)"
    );
}

// ─── Online tests (require internet) ─────────────────────────────────────────

/// Diagnostic: fetch real Wikipedia HTML and trace why clean_html may return empty.
/// Run with: cargo test -- --include-ignored test_diagnose_wikipedia_html
#[tokio::test]
#[ignore = "requires internet access"]
async fn test_diagnose_wikipedia_html() {
    use scraper::{Html, Selector};

    let client = reqwest::Client::new();
    let html = client
        .get("https://en.wikipedia.org/wiki/Rust_(programming_language)")
        .header("User-Agent", "Mozilla/5.0 (compatible; Fetchless/0.1)")
        .send()
        .await
        .expect("request failed")
        .text()
        .await
        .expect("body failed");

    println!("=== HTML size: {} bytes ===", html.len());

    let doc = Html::parse_document(&html);

    // Check which container selector matches
    let selectors = [
        r#"div[id="mw-content-text"], div[id="main-content"]"#,
        "article",
        "main",
        r#"div[role="main"]"#,
    ];
    for sel_str in &selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = doc.select(&sel).next() {
                println!("=== Container matched: '{sel_str}' → <{} id={:?} class={:?}> ===",
                    el.value().name(),
                    el.value().id(),
                    el.value().attr("class"));
                // Print ancestor chain
                let mut current = el.parent();
                let mut depth = 0;
                while let Some(node) = current {
                    if let Some(parent_el) = scraper::ElementRef::wrap(node) {
                        let v = parent_el.value();
                        println!("  ancestor[{depth}]: <{} id={:?} class={:?}>",
                            v.name(), v.id(), v.attr("class"));
                    }
                    current = node.parent();
                    depth += 1;
                    if depth > 10 { break; }
                }
                break;
            }
        }
    }

    // Run the actual clean_html
    let cleaned = html_cleaner::clean_html(&html);
    println!("=== Cleaned length: {} chars ===", cleaned.len());
    if !cleaned.is_empty() {
        println!("=== First 300 chars: {} ===", &cleaned[..cleaned.len().min(300)]);
    } else {
        println!("=== Cleaned output is EMPTY ===");
    }

    assert!(!cleaned.trim().is_empty(),
        "clean_html produced empty output for real Wikipedia HTML");
}

/// Fetch a real Wikipedia article and verify content is non-empty.
/// Run with: cargo test -- --include-ignored test_fetch_wikipedia_real
#[tokio::test]
#[ignore = "requires internet access"]
async fn test_fetch_wikipedia_real() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": "https://en.wikipedia.org/wiki/Rust_(programming_language)"}))
        .await;
    res.assert_status_ok();

    let body: Value = res.json();
    let content = body["content"].as_str().unwrap_or("");
    assert!(
        !content.is_empty(),
        "Wikipedia content should not be empty after cleaning.\nFull response: {body:#?}"
    );
    assert!(
        body["original_tokens"].as_u64().unwrap_or(0) > 0,
        "original_tokens should be > 0"
    );
    assert!(
        body["reduction_pct"].as_f64().unwrap_or(0.0) > 50.0,
        "Wikipedia should yield >50% token reduction, got {}%",
        body["reduction_pct"].as_f64().unwrap_or(0.0)
    );
}

/// Fetch a simple page (httpbin) and verify response structure.
/// Run with: cargo test -- --include-ignored test_fetch_httpbin_html
#[tokio::test]
#[ignore = "requires internet access"]
async fn test_fetch_httpbin_html() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": "https://httpbin.org/html"}))
        .await;
    res.assert_status_ok();

    let body: Value = res.json();
    assert_eq!(body["url"], "https://httpbin.org/html");
    assert!(!body["content"].as_str().unwrap_or("").is_empty(), "content should not be empty");
    assert!(body["original_tokens"].as_u64().unwrap_or(0) > 0);
    assert!(body["cleaned_tokens"].as_u64().unwrap_or(0) > 0);
    assert!(body["reduction_pct"].as_f64().unwrap_or(0.0) >= 0.0);
    assert!(!body["from_cache"].as_bool().unwrap_or(true), "first fetch should not be from cache");
}

/// Fetch the same URL twice and verify the second hit comes from cache.
/// Run with: cargo test -- --include-ignored test_fetch_cache_hit
#[tokio::test]
#[ignore = "requires internet access"]
async fn test_fetch_cache_hit() {
    let server = make_server();
    let payload = json!({"url": "https://httpbin.org/html", "ttl": 300});

    // First fetch — not from cache
    let first: Value = server.post("/fetch").json(&payload).await.json();
    assert!(!first["from_cache"].as_bool().unwrap_or(true), "first fetch must not be cached");

    // Second fetch — must hit cache
    let second: Value = server.post("/fetch").json(&payload).await.json();
    assert!(second["from_cache"].as_bool().unwrap_or(false), "second fetch must be from cache");
    assert_eq!(first["content"], second["content"], "cached content must match original");
}

/// Fetch a JSON API endpoint and verify it is cleaned correctly.
/// Run with: cargo test -- --include-ignored test_fetch_json_endpoint
#[tokio::test]
#[ignore = "requires internet access"]
async fn test_fetch_json_endpoint() {
    let server = make_server();
    let res = server.post("/fetch")
        .json(&json!({"url": "https://httpbin.org/json"}))
        .await;
    res.assert_status_ok();

    let body: Value = res.json();
    assert!(!body["content"].as_str().unwrap_or("").is_empty());
    // JSON responses are labelled as "json"
    assert_eq!(body["content_type"], "json");
}

/// Batch fetch multiple real URLs.
/// Run with: cargo test -- --include-ignored test_fetch_batch_real
#[tokio::test]
#[ignore = "requires internet access"]
async fn test_fetch_batch_real() {
    let server = make_server();
    let res = server.post("/fetch/batch")
        .json(&json!({
            "urls": [
                "https://httpbin.org/html",
                "https://httpbin.org/json"
            ]
        }))
        .await;
    res.assert_status_ok();

    let body: Value = res.json();
    let results = body["results"].as_array().expect("results array");
    assert_eq!(results.len(), 2, "should return 2 results");
    assert!(body["total_original_tokens"].as_u64().unwrap_or(0) > 0);
    assert!(body["total_reduction_pct"].as_f64().unwrap_or(0.0) >= 0.0);

    for (i, r) in results.iter().enumerate() {
        assert!(r["error"].as_str().map_or(true, |e| e.is_empty()), "result {i} has error: {}", r["error"]);
        assert!(!r["content"].as_str().unwrap_or("").is_empty(), "result {i} should have content");
    }
}
