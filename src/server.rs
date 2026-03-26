use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::Config;
use crate::data_proxy;
use crate::db::{self, Db};
use crate::error::AppError;
use crate::optimizer;
use crate::url_validator;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub client: reqwest::Client,
    pub config: Config,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/refine", post(refine))
        .route("/fetch", post(fetch))
        .route("/fetch/batch", post(fetch_batch))
        .route("/stats", get(stats))
        .with_state(Arc::new(state))
}

// ============================================================
//  HOME
// ============================================================

async fn home(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let stats = db::get_stats(&state.db);
    Json(serde_json::json!({
        "name": "Fetchless",
        "version": env!("CARGO_PKG_VERSION"),
        "layers": {
            "1": "Prompt Refiner (opt-in) - /refine",
            "2": "Data Proxy (active) - /fetch, /fetch/batch",
        },
        "endpoints": {
            "/refine": "POST {\"text\": \"...\"}",
            "/fetch": "POST {\"url\": \"https://...\"}",
            "/fetch/batch": "POST {\"urls\": [\"...\", \"...\"]}",
            "/stats": "GET"
        },
        "stats": stats,
    }))
}

// ============================================================
//  REFINE
// ============================================================

#[derive(Deserialize)]
struct RefineRequest {
    text: Option<String>,
    prompt: Option<String>,
}

#[derive(Serialize)]
struct RefineResponse {
    original: String,
    suggested: String,
    original_tokens: usize,
    suggested_tokens: usize,
    savings_pct: f64,
    confidence: f64,
    protected_entities: Vec<String>,
}

async fn refine(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RefineRequest>,
) -> Result<Json<RefineResponse>, AppError> {
    let text = body
        .text
        .or(body.prompt)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Send {\"text\": \"your prompt\"}".to_string()))?;

    let result = optimizer::optimize_prompt_default(&text);

    let savings = if result.original_tokens > 0 {
        ((1.0 - result.optimized_tokens as f64 / result.original_tokens as f64) * 1000.0).round()
            / 10.0
    } else {
        0.0
    };

    let rid = uuid::Uuid::new_v4().to_string()[..8].to_string();
    db::log_prompt(
        &state.db,
        &rid,
        result.original_tokens,
        result.optimized_tokens,
        result.confidence,
        result.sent_optimized,
        &result.skip_reason,
    );

    tracing::info!(
        request_id = %rid,
        original_tokens = result.original_tokens,
        optimized_tokens = result.optimized_tokens,
        savings_pct = savings,
        "REFINE"
    );

    Ok(Json(RefineResponse {
        original: result.original,
        suggested: result.optimized,
        original_tokens: result.original_tokens,
        suggested_tokens: result.optimized_tokens,
        savings_pct: savings,
        confidence: (result.confidence * 100.0).round() / 100.0,
        protected_entities: result.protected_entities,
    }))
}

// ============================================================
//  FETCH
// ============================================================

#[derive(Deserialize)]
struct FetchRequest {
    url: Option<String>,
    ttl: Option<u64>,
}

#[derive(Serialize)]
struct FetchResponse {
    url: String,
    content: String,
    content_type: String,
    original_tokens: usize,
    cleaned_tokens: usize,
    reduction_pct: f64,
    from_cache: bool,
}

async fn fetch(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FetchRequest>,
) -> Result<Json<FetchResponse>, AppError> {
    let url = body
        .url
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Validation("Send {\"url\": \"https://...\"}".to_string()))?;

    let ttl = body.ttl.unwrap_or(state.config.default_ttl);
    let result = data_proxy::fetch_and_clean(&state.db, &state.client, &url, ttl).await;

    if !result.error.is_empty() {
        return Err(AppError::Fetch(result.error));
    }

    let reduction = if result.original_tokens > 0 {
        ((1.0 - result.cleaned_tokens as f64 / result.original_tokens as f64) * 1000.0).round()
            / 10.0
    } else {
        0.0
    };

    tracing::info!(
        url = %url,
        original_tokens = result.original_tokens,
        cleaned_tokens = result.cleaned_tokens,
        reduction_pct = reduction,
        from_cache = result.from_cache,
        "FETCH"
    );

    Ok(Json(FetchResponse {
        url: result.url,
        content: result.content,
        content_type: result.content_type,
        original_tokens: result.original_tokens,
        cleaned_tokens: result.cleaned_tokens,
        reduction_pct: reduction,
        from_cache: result.from_cache,
    }))
}

// ============================================================
//  FETCH BATCH
// ============================================================

#[derive(Deserialize)]
struct FetchBatchRequest {
    urls: Option<Vec<String>>,
    ttl: Option<u64>,
}

#[derive(Serialize)]
struct BatchItem {
    url: String,
    content: String,
    content_type: String,
    original_tokens: usize,
    cleaned_tokens: usize,
    from_cache: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
}

#[derive(Serialize)]
struct FetchBatchResponse {
    results: Vec<BatchItem>,
    total_original_tokens: usize,
    total_cleaned_tokens: usize,
    total_reduction_pct: f64,
}

async fn fetch_batch(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FetchBatchRequest>,
) -> Result<Json<FetchBatchResponse>, AppError> {
    let urls = body
        .urls
        .filter(|u| !u.is_empty())
        .ok_or_else(|| {
            AppError::Validation("Send {\"urls\": [\"https://...\", ...]}".to_string())
        })?;

    url_validator::validate_batch(&urls).await?;

    let ttl = body.ttl.unwrap_or(state.config.default_ttl);
    let mut results = Vec::new();
    let mut total_original = 0usize;
    let mut total_cleaned = 0usize;

    for url in &urls {
        let r = data_proxy::fetch_and_clean(&state.db, &state.client, url, ttl).await;
        total_original += r.original_tokens;
        total_cleaned += r.cleaned_tokens;
        results.push(BatchItem {
            url: r.url,
            content: r.content,
            content_type: r.content_type,
            original_tokens: r.original_tokens,
            cleaned_tokens: r.cleaned_tokens,
            from_cache: r.from_cache,
            error: r.error,
        });
    }

    let total_reduction = if total_original > 0 {
        ((1.0 - total_cleaned as f64 / total_original as f64) * 1000.0).round() / 10.0
    } else {
        0.0
    };

    tracing::info!(
        count = urls.len(),
        total_original_tokens = total_original,
        total_cleaned_tokens = total_cleaned,
        total_reduction_pct = total_reduction,
        "BATCH"
    );

    Ok(Json(FetchBatchResponse {
        results,
        total_original_tokens: total_original,
        total_cleaned_tokens: total_cleaned,
        total_reduction_pct: total_reduction,
    }))
}

// ============================================================
//  STATS
// ============================================================

async fn stats(State(state): State<Arc<AppState>>) -> Json<db::Stats> {
    Json(db::get_stats(&state.db))
}
