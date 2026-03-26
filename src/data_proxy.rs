use crate::db::{self, Db};
use crate::html_cleaner::clean_html;
use crate::json_cleaner::clean_json_response;
use crate::token::estimate_tokens;
use crate::url_validator::validate_url;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DataResult {
    pub url: String,
    pub original_size: usize,
    pub cleaned_size: usize,
    pub original_tokens: usize,
    pub cleaned_tokens: usize,
    pub from_cache: bool,
    pub content: String,
    pub content_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

pub async fn fetch_and_clean(
    db: &Db,
    client: &reqwest::Client,
    url: &str,
    ttl: u64,
) -> DataResult {
    // Validate URL (SSRF protection)
    if let Err(e) = validate_url(url).await {
        let error = e.to_string();
        db::log_fetch(db, url, 0, 0, false, &error);
        return DataResult {
            url: url.to_string(),
            original_size: 0,
            cleaned_size: 0,
            original_tokens: 0,
            cleaned_tokens: 0,
            from_cache: false,
            content: String::new(),
            content_type: "error".to_string(),
            error,
        };
    }

    // Check cache
    if let Some(entry) = db::cache_get(db, url) {
        db::log_fetch(
            db,
            url,
            entry.original_tokens as usize,
            entry.cleaned_tokens as usize,
            true,
            "",
        );
        return DataResult {
            url: url.to_string(),
            original_size: entry.original_tokens as usize * 4,
            cleaned_size: entry.cleaned_tokens as usize * 4,
            original_tokens: entry.original_tokens as usize,
            cleaned_tokens: entry.cleaned_tokens as usize,
            from_cache: true,
            content: entry.payload,
            content_type: "cached".to_string(),
            error: String::new(),
        };
    }

    // Fetch
    let response = match client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; Fetchless/0.1)",
        )
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let error = e.to_string();
            db::log_fetch(db, url, 0, 0, false, &error);
            return DataResult {
                url: url.to_string(),
                original_size: 0,
                cleaned_size: 0,
                original_tokens: 0,
                cleaned_tokens: 0,
                from_cache: false,
                content: String::new(),
                content_type: "error".to_string(),
                error,
            };
        }
    };

    // Check for HTTP errors
    if let Err(e) = response.error_for_status_ref() {
        let error = e.to_string();
        db::log_fetch(db, url, 0, 0, false, &error);
        return DataResult {
            url: url.to_string(),
            original_size: 0,
            cleaned_size: 0,
            original_tokens: 0,
            cleaned_tokens: 0,
            from_cache: false,
            content: String::new(),
            content_type: "error".to_string(),
            error,
        };
    }

    let content_type_header = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let raw = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            let error = e.to_string();
            db::log_fetch(db, url, 0, 0, false, &error);
            return DataResult {
                url: url.to_string(),
                original_size: 0,
                cleaned_size: 0,
                original_tokens: 0,
                cleaned_tokens: 0,
                from_cache: false,
                content: String::new(),
                content_type: "error".to_string(),
                error,
            };
        }
    };

    let original_tokens = estimate_tokens(&raw);

    // Route to correct cleaner
    let (cleaned, ctype) = if content_type_header.contains("json") {
        (clean_json_response(&raw), "json".to_string())
    } else if content_type_header.contains("html") || raw.trim_start().starts_with('<') {
        (clean_html(&raw), "html_cleaned".to_string())
    } else {
        (raw.trim().to_string(), "text".to_string())
    };

    let cleaned_tokens = estimate_tokens(&cleaned);

    // Cache
    db::cache_set(db, url, &cleaned, original_tokens, cleaned_tokens, ttl);

    // Log
    db::log_fetch(db, url, original_tokens, cleaned_tokens, false, "");

    DataResult {
        url: url.to_string(),
        original_size: raw.len(),
        cleaned_size: cleaned.len(),
        original_tokens,
        cleaned_tokens,
        from_cache: false,
        content: cleaned,
        content_type: ctype,
        error: String::new(),
    }
}
