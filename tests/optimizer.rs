use fetchless::optimizer::{extract_protected, is_code_or_json, optimize_prompt_default};

#[test]
fn test_verbose_financial_query_optimized() {
    let text = "Hey, so I was wondering if you could maybe look at Apple's \
        latest earnings report and tell me how they did compared to \
        last quarter, especially the services revenue part because \
        I've been tracking that, and also what analysts are saying. \
        Thanks in advance, I really appreciate it!";
    let result = optimize_prompt_default(text);
    assert!(result.sent_optimized);
    assert!(result.optimized_tokens < result.original_tokens);
}

#[test]
fn test_negations_preserved() {
    let text = "Hi there, could you please analyze TSLA stock performance \
        but don't include any crypto-related comparisons. Also, \
        remember what we discussed last time about the $45 price \
        target — I want to revisit that. Thank you so much!";
    let result = optimize_prompt_default(text);
    assert!(result.sent_optimized);
    let opt_lower = result.optimized.to_lowercase();
    assert!(opt_lower.contains("don't") || opt_lower.contains("don't"));
}

#[test]
fn test_short_prompt_skipped() {
    let text = "Compare AAPL and MSFT Q3 earnings.";
    let result = optimize_prompt_default(text);
    assert!(!result.sent_optimized);
    assert_eq!(result.skip_reason, "below_min_tokens");
}

#[test]
fn test_code_skipped() {
    let text = "def calculate_roi(investment, returns):\n    import numpy as np\n    return np.sum(returns) / investment\nFix this function to handle edge cases.";
    let result = optimize_prompt_default(text);
    assert!(!result.sent_optimized);
    assert_eq!(result.skip_reason, "code_or_json");
}

#[test]
fn test_heavy_filler_optimized() {
    let text = "Okay so basically I was just thinking, honestly, could you \
        kind of like help me understand what the PE ratio actually \
        means for a company like NVDA? I mean, I know it's sort of \
        important but I don't really get why it matters so much. \
        If it's not too much trouble, that would be awesome. Cheers!";
    let result = optimize_prompt_default(text);
    assert!(result.sent_optimized);
    assert!(result.optimized_tokens < result.original_tokens);
}

#[test]
fn test_conversation_refs_preserved() {
    let text = "Hey, so going back to what we discussed earlier about my \
        portfolio allocation, I think I want to shift more into \
        bonds. Like I mentioned last time, I'm not comfortable with \
        the 80/20 split anymore. Can you suggest a more conservative \
        approach? Thanks!";
    let result = optimize_prompt_default(text);
    assert!(result.sent_optimized);
    let opt_lower = result.optimized.to_lowercase();
    assert!(
        opt_lower.contains("what we discussed")
            || opt_lower.contains("like i mentioned")
            || opt_lower.contains("my portfolio")
    );
}

#[test]
fn test_multiple_tickers_and_money() {
    let text = "Hi, I would really like you to compare the performance of \
        AAPL, MSFT, GOOGL, and AMZN over the last year. Specifically, \
        which one had the best return if I invested $10,000 in each? \
        Also, what's the YTD performance? I'd appreciate it, thanks!";
    let result = optimize_prompt_default(text);
    assert!(result.sent_optimized);
    assert!(result.protected_entities.iter().any(|e| e == "AAPL"));
    assert!(result.protected_entities.iter().any(|e| e == "$10,000"));
}

#[test]
fn test_is_code_or_json() {
    assert!(is_code_or_json("{ \"key\": \"val\" } import foo class Bar"));
    assert!(!is_code_or_json("Tell me about AAPL stock"));
}

#[test]
fn test_extract_protected_filler_negations_excluded() {
    let text = "If it's not too much trouble, don't skip AAPL";
    let protected = extract_protected(text);
    let negation_texts: Vec<&str> = protected
        .iter()
        .filter(|e| e.kind == "negation")
        .map(|e| e.text.as_str())
        .collect();
    // "not" inside filler phrase should be excluded, but "don't" should remain
    assert!(negation_texts.contains(&"don't"));
}
