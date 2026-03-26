/// Rough token estimate: ~4 chars per token.
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}
