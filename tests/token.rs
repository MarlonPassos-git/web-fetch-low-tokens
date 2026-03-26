use fetchless::token::estimate_tokens;

#[test]
fn test_estimate_tokens_basic() {
    assert_eq!(estimate_tokens("hello world!"), 3); // 12 / 4
}

#[test]
fn test_estimate_tokens_empty() {
    assert_eq!(estimate_tokens(""), 1); // min 1
}

#[test]
fn test_estimate_tokens_short() {
    assert_eq!(estimate_tokens("hi"), 1); // 2 / 4 = 0, clamped to 1
}

#[test]
fn test_estimate_tokens_long() {
    let text = "a".repeat(400);
    assert_eq!(estimate_tokens(&text), 100);
}
