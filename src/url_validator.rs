use std::net::IpAddr;
use url::Url;

use crate::error::AppError;

const MAX_BATCH_URLS: usize = 10;

/// Blocked IP networks for SSRF protection
struct BlockedNetwork {
    addr: IpAddr,
    prefix_len: u8,
}

impl BlockedNetwork {
    fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(check)) => {
                let net_bits = u32::from(net);
                let check_bits = u32::from(check);
                let mask = if self.prefix_len == 0 {
                    0
                } else {
                    !0u32 << (32 - self.prefix_len)
                };
                (net_bits & mask) == (check_bits & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(check)) => {
                let net_bits = u128::from(net);
                let check_bits = u128::from(check);
                let mask = if self.prefix_len == 0 {
                    0
                } else {
                    !0u128 << (128 - self.prefix_len)
                };
                (net_bits & mask) == (check_bits & mask)
            }
            _ => false,
        }
    }
}

fn blocked_networks() -> Vec<BlockedNetwork> {
    use std::net::{Ipv4Addr, Ipv6Addr};
    vec![
        BlockedNetwork { addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 0)), prefix_len: 8 },
        BlockedNetwork { addr: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)), prefix_len: 8 },
        BlockedNetwork { addr: IpAddr::V4(Ipv4Addr::new(172, 16, 0, 0)), prefix_len: 12 },
        BlockedNetwork { addr: IpAddr::V4(Ipv4Addr::new(192, 168, 0, 0)), prefix_len: 16 },
        BlockedNetwork { addr: IpAddr::V4(Ipv4Addr::new(169, 254, 0, 0)), prefix_len: 16 },
        BlockedNetwork { addr: IpAddr::V4(Ipv4Addr::new(100, 64, 0, 0)), prefix_len: 10 },
        BlockedNetwork { addr: IpAddr::V6(Ipv6Addr::LOCALHOST), prefix_len: 128 },
        BlockedNetwork { addr: IpAddr::V6(Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0)), prefix_len: 7 },
        BlockedNetwork { addr: IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0)), prefix_len: 10 },
    ]
}

fn check_ip_blocked(ip: IpAddr) -> Result<(), AppError> {
    for network in blocked_networks() {
        if network.contains(ip) {
            return Err(AppError::Validation(format!(
                "URL resolves to a private/reserved address ({ip}). SSRF protection rejected this request."
            )));
        }
    }
    Ok(())
}

/// Validate that a URL is a public HTTPS target.
/// Performs DNS resolution and checks IPs against blocked ranges.
pub async fn validate_url(url_str: &str) -> Result<String, AppError> {
    if url_str.trim().is_empty() {
        return Err(AppError::Validation(
            "URL must be a non-empty string.".to_string(),
        ));
    }

    let parsed = Url::parse(url_str)
        .map_err(|e| AppError::Validation(format!("Invalid URL: {e}")))?;

    if parsed.scheme() != "https" {
        return Err(AppError::Validation(format!(
            "Only HTTPS URLs are allowed (got scheme: '{}').",
            parsed.scheme()
        )));
    }

    let hostname = parsed
        .host_str()
        .ok_or_else(|| AppError::Validation("URL has no hostname.".to_string()))?;

    // Try to parse hostname directly as IP first
    if let Ok(ip) = hostname.parse::<IpAddr>() {
        check_ip_blocked(ip)?;
        return Ok(url_str.to_string());
    }

    // DNS resolution
    let resolver = hickory_resolver::TokioResolver::builder_tokio()
        .map_err(|e| AppError::Validation(format!("DNS resolver error: {e}")))?
        .build();

    let response = resolver
        .lookup_ip(hostname)
        .await
        .map_err(|e| AppError::Validation(format!("Cannot resolve hostname '{hostname}': {e}")))?;

    for ip in response.iter() {
        check_ip_blocked(ip)?;
    }

    Ok(url_str.to_string())
}

/// Validate a batch of URLs.
pub async fn validate_batch(urls: &[String]) -> Result<(), AppError> {
    if urls.len() > MAX_BATCH_URLS {
        return Err(AppError::Validation(format!(
            "Batch size {} exceeds maximum of {MAX_BATCH_URLS}.",
            urls.len()
        )));
    }
    for url in urls {
        validate_url(url).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rejects_http() {
        let result = validate_url("http://example.com").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HTTPS"));
    }

    #[tokio::test]
    async fn test_rejects_empty() {
        let result = validate_url("").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejects_no_scheme() {
        let result = validate_url("not-a-url").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejects_loopback_ip() {
        let result = validate_url("https://127.0.0.1/foo").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private/reserved"));
    }

    #[tokio::test]
    async fn test_rejects_private_10() {
        let result = validate_url("https://10.0.0.1/foo").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejects_private_172() {
        let result = validate_url("https://172.16.0.1/foo").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejects_private_192() {
        let result = validate_url("https://192.168.1.1/foo").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rejects_link_local() {
        let result = validate_url("https://169.254.169.254/latest/meta-data/").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_accepts_valid_https() {
        let result = validate_url("https://example.com").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_batch_limit() {
        let urls: Vec<String> = (0..11).map(|i| format!("https://example{i}.com")).collect();
        let result = validate_batch(&urls).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[test]
    fn test_blocked_network_contains() {
        use std::net::Ipv4Addr;
        let net = BlockedNetwork {
            addr: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)),
            prefix_len: 8,
        };
        assert!(net.contains(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(!net.contains(IpAddr::V4(Ipv4Addr::new(11, 0, 0, 1))));
    }
}
