use fetchless::html_cleaner::clean_html;

#[test]
fn test_removes_script_and_style() {
    let html = r#"<html><body><script>alert('xss')</script><style>.x{}</style><p>Hello</p></body></html>"#;
    let result = clean_html(html);
    assert!(result.contains("Hello"));
    assert!(!result.contains("alert"));
    assert!(!result.contains(".x{}"));
}

#[test]
fn test_removes_nav_footer() {
    let html = r#"<html><body><nav>Menu items</nav><main><p>Content here</p></main><footer>Copyright</footer></body></html>"#;
    let result = clean_html(html);
    assert!(result.contains("Content here"));
    assert!(!result.contains("Menu items"));
    assert!(!result.contains("Copyright"));
}

#[test]
fn test_removes_noise_classes() {
    let html = r#"<html><body><article><p>Good content</p><div class="ad-banner">Buy now!</div></article></body></html>"#;
    let result = clean_html(html);
    assert!(result.contains("Good content"));
    assert!(!result.contains("Buy now!"));
}

#[test]
fn test_prefers_article_container() {
    let html = r#"<html><body><div class="sidebar">Side content</div><article><p>Main article</p></article></body></html>"#;
    let result = clean_html(html);
    assert!(result.contains("Main article"));
}

#[test]
fn test_empty_html() {
    let result = clean_html("<html><body></body></html>");
    assert!(result.trim().is_empty());
}

#[test]
fn test_plain_text_in_html() {
    let html = "<html><body><p>Just some plain text here.</p></body></html>";
    let result = clean_html(html);
    assert!(result.contains("Just some plain text here."));
}
