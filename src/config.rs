use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "fetchless", version, about = "Token-optimized web proxy for AI agents")]
pub struct Config {
    /// Port to listen on
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Address to bind to
    #[arg(long, default_value = "127.0.0.1")]
    pub bind: String,

    /// Path to SQLite database
    #[arg(long, default_value = "agent_proxy.db")]
    pub db_path: String,

    /// Default cache TTL in seconds
    #[arg(long, default_value_t = 300)]
    pub default_ttl: u64,

    /// Run MCP server instead of HTTP
    #[arg(long)]
    pub mcp: bool,
}
