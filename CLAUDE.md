# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**fetchless** — a Rust binary that strips web pages down to clean text before they enter an AI agent's context window. Single binary, no dependencies, cross-platform. No API keys, no LLM, no GPU. Pure rule-based processing.

## Commands

```bash
# Build
cargo build
cargo build --release

# Run server (port 8080 by default)
cargo run -- --port 8080

# Run as MCP server (stdio transport)
cargo run -- --mcp

# Tests (unit + integration, offline)
cargo test

# Tests including live URLs (requires internet)
cargo test -- --include-ignored

# Lint / format
cargo clippy -- -D warnings
cargo fmt --check
```

## Architecture

Two-layer system with an optional MCP interface:

- **Layer 1 — Prompt Refiner** (`src/optimizer.rs`): Rule-based filler word removal. Protects tickers, money values, time refs, negations, and conversation references via regex patterns. Confidence scoring ensures protected entities survive optimization. Opt-in via `/refine` endpoint. `fancy-regex` used only for the ticker lookahead pattern.

- **Layer 2 — Data Proxy** (`src/data_proxy.rs`): Fetches URLs via `reqwest`, routes to `html_cleaner` or `json_cleaner` based on content-type. SQLite cache with configurable TTL. This is the main value — 86-99% token reduction on web pages.

- **URL Validator** (`src/url_validator.rs`): SSRF protection layer. Only allows HTTPS URLs resolving to public IPs. Blocks RFC 1918, loopback, link-local, CGNAT, and other reserved ranges. DNS via `hickory-resolver`. Batch limit of 10 URLs.

- **HTML Cleaner** (`src/html_cleaner.rs`): Uses `scraper` (HTML5 parser) to find the best content container (`article`, `main`, `div[role=main]`, Wikipedia `mw-content-text`), then walks the `ego-tree` skipping noise tags and noisy CSS classes.

- **JSON Cleaner** (`src/json_cleaner.rs`): Recursively removes junk keys (meta, ads, tracking, etc.) via `serde_json`.

- **REST Server** (`src/server.rs`): Axum router with `AppState` (db, http_client, config). Endpoints: `GET /`, `POST /refine`, `POST /fetch`, `POST /fetch/batch`, `GET /stats`.

- **MCP Server** (`src/mcp.rs`): Uses `rmcp` SDK. Exposes `fetch_clean`, `fetch_clean_batch`, and `refine_prompt` as MCP tools.

- **Database** (`src/db.rs`): SQLite via `rusqlite` (bundled). Three tables: `prompt_log`, `data_cache`, `data_log`. Cache get/set with TTL, stats aggregation.

- **Config** (`src/config.rs`): CLI args via `clap` derive — `--port`, `--bind`, `--db-path`, `--default-ttl`, `--mcp`.

## Source Layout

```
src/
├── main.rs          # Entry point: parse config, init tracing, start server or MCP
├── config.rs        # CLI args (clap derive)
├── error.rs         # AppError enum + axum IntoResponse
├── token.rs         # estimate_tokens(text) = len / 4
├── optimizer.rs     # Layer 1: filler removal + entity protection
├── url_validator.rs # SSRF protection (HTTPS only, IP blocking, DNS)
├── html_cleaner.rs  # HTML → clean text (scraper + ego-tree)
├── json_cleaner.rs  # JSON junk key removal (serde_json)
├── data_proxy.rs    # Layer 2: fetch + route + cache
├── db.rs            # SQLite: init, cache, logging, stats
├── server.rs        # Axum router + handlers
└── mcp.rs           # MCP server (rmcp)
```

## Key Design Decisions

- Token estimation uses `len(text) / 4` (rough ~4 chars/token approximation)
- `is_code_or_json` check runs before `below_min_tokens` check in optimizer — code is always skipped regardless of length
- HTML cleaning: container search order is `mw-content-text` → `main-content` → `article` → `main` → `div[role=main]`
- Server binds to `127.0.0.1` only by default (not `0.0.0.0`) for security; override with `--bind`
- All URLs must be HTTPS — HTTP is rejected at the validator level
- SQLite is used for both caching and request logging (default path: `fetchless.db`)
- `ego-tree = "0.10"` must match the version used by `scraper 0.22` to avoid type conflicts
- MCP tool functions return `Content` directly (not `Result<Content, Error>`) since errors are embedded in the content text

## API Contract

```
GET  /              → server info + stats
POST /refine        → {"text": "..."} → {original, suggested, savings_pct, confidence, protected_entities}
POST /fetch         → {"url": "...", "ttl": 300} → {url, content, content_type, original_tokens, cleaned_tokens, reduction_pct}
POST /fetch/batch   → {"urls": [...], "ttl": 300} → {results: [...], total_*}
GET  /stats         → {layer1_*, layer2_*, total_tokens_saved, est_cost_saved}
```
