use fetchless::url_validator::{validate_batch, validate_url, BlockedNetwork};
use std::net::IpAddr;

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
