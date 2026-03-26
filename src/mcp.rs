use rmcp::{ServiceExt, ServerHandler, model::*, tool};

use crate::data_proxy;
use crate::db::Db;
use crate::optimizer;

#[derive(Clone)]
pub struct FetchlessMcp {
    db: Db,
    client: reqwest::Client,
}

#[tool(tool_box)]
impl FetchlessMcp {
    pub fn new(db: Db, client: reqwest::Client) -> Self {
        Self { db, client }
    }

    /// Fetch a URL and return clean text with all HTML noise removed.
    /// Strips navigation, ads, scripts, styles, and boilerplate.
    /// Caches results to avoid redundant fetches.
    /// Typical reduction: 86-99% fewer tokens than raw HTML.
    #[tool(
        name = "fetch_clean",
        description = "Fetch a URL and return clean text with HTML noise removed"
    )]
    async fn fetch_clean(
        &self,
        #[tool(param)] url: String,
        #[tool(param)] ttl: Option<u64>,
    ) -> Content {
        let ttl = ttl.unwrap_or(300);
        let result = data_proxy::fetch_and_clean(&self.db, &self.client, &url, ttl).await;

        if !result.error.is_empty() {
            return Content::text(format!("Error fetching {url}: {}", result.error));
        }

        let reduction = if result.original_tokens > 0 {
            ((1.0 - result.cleaned_tokens as f64 / result.original_tokens as f64) * 1000.0)
                .round()
                / 10.0
        } else {
            0.0
        };

        let mut header = format!(
            "[Fetchless] {} -> {} tokens ({reduction}% reduced)",
            result.original_tokens, result.cleaned_tokens
        );
        if result.from_cache {
            header.push_str(" [cached]");
        }

        Content::text(format!("{header}\n\n{}", result.content))
    }

    /// Fetch multiple URLs and return clean text for each.
    #[tool(
        name = "fetch_clean_batch",
        description = "Fetch multiple URLs and return clean text for each"
    )]
    async fn fetch_clean_batch(
        &self,
        #[tool(param)] urls: Vec<String>,
        #[tool(param)] ttl: Option<u64>,
    ) -> Content {
        let ttl = ttl.unwrap_or(300);
        let mut parts = Vec::new();
        let mut total_original = 0usize;
        let mut total_cleaned = 0usize;

        for url in &urls {
            let r = data_proxy::fetch_and_clean(&self.db, &self.client, url, ttl).await;
            total_original += r.original_tokens;
            total_cleaned += r.cleaned_tokens;
            if !r.error.is_empty() {
                parts.push(format!("--- {url} ---\nError: {}\n", r.error));
            } else {
                parts.push(format!("--- {url} ---\n{}\n", r.content));
            }
        }

        let reduction = if total_original > 0 {
            ((1.0 - total_cleaned as f64 / total_original as f64) * 1000.0).round() / 10.0
        } else {
            0.0
        };

        let header = format!(
            "[Fetchless] {} URLs | {} -> {} tokens ({reduction}% reduced)\n\n",
            urls.len(),
            total_original,
            total_cleaned
        );

        Content::text(format!("{header}{}", parts.join("\n")))
    }

    /// Refine a prompt by removing filler words while preserving entities,
    /// negations, time references, and conversation context.
    #[tool(
        name = "refine_prompt",
        description = "Refine a prompt by removing filler words while preserving key entities"
    )]
    async fn refine_prompt(
        &self,
        #[tool(param)] text: String,
    ) -> Content {
        let result = optimizer::optimize_prompt_default(&text);

        let savings = if result.original_tokens > 0 {
            ((1.0 - result.optimized_tokens as f64 / result.original_tokens as f64) * 1000.0)
                .round()
                / 10.0
        } else {
            0.0
        };

        let mut output = format!(
            "Original ({} tokens):\n{}\n\n",
            result.original_tokens, result.original
        );

        if result.sent_optimized {
            output.push_str(&format!(
                "Suggested ({} tokens, {savings}% smaller):\n{}\n\n",
                result.optimized_tokens, result.optimized
            ));
            let entities: Vec<&str> = result
                .protected_entities
                .iter()
                .take(8)
                .map(|s| s.as_str())
                .collect();
            output.push_str(&format!("Protected: {}\n", entities.join(", ")));
            output.push_str(&format!("Confidence: {:.0}%", result.confidence * 100.0));
        } else {
            output.push_str(&format!(
                "No optimization needed. Reason: {}",
                result.skip_reason
            ));
        }

        Content::text(output)
    }
}

#[tool(tool_box)]
impl ServerHandler for FetchlessMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: None,
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "fetchless".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "Token-optimized web proxy — fetch clean web content and refine prompts"
                    .to_string(),
            ),
        }
    }
}

pub async fn run_mcp(db: Db, client: reqwest::Client) -> anyhow::Result<()> {
    let server = FetchlessMcp::new(db, client);
    let transport = rmcp::transport::io::stdio();
    let ct = server.serve(transport).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    ct.waiting().await?;
    Ok(())
}
