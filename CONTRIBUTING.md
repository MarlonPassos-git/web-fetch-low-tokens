# Contributing to Fetchless

## Setup

Requer [Rust](https://rustup.rs/) stable 1.75+.

```bash
git clone <repo>
cd web-fetch-low-tokens
cargo build
```

## Testes

```bash
# Unitários + integração (offline)
cargo test

# Incluir testes com URLs reais (requer internet)
cargo test -- --include-ignored
```

## Lint e formatação

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

## Estrutura do projeto

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

## PRs

- Rode `cargo test` e `cargo clippy -- -D warnings` antes de abrir PR
- Testes novos devem ser offline por padrão; marque com `#[ignore]` se precisarem de internet
