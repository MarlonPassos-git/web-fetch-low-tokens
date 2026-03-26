use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use std::sync::LazyLock;

use crate::token::estimate_tokens;

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizationResult {
    pub original: String,
    pub optimized: String,
    pub original_tokens: usize,
    pub optimized_tokens: usize,
    pub confidence: f64,
    pub protected_entities: Vec<String>,
    pub sent_optimized: bool,
    pub skip_reason: String,
}

#[derive(Debug, Clone)]
pub struct ProtectedEntity {
    pub text: String,
    pub kind: String,
}

// ============================================================
//  PROTECTED PATTERNS
// ============================================================

static TICKER_PATTERN: LazyLock<FancyRegex> =
    LazyLock::new(|| FancyRegex::new(r"\b[A-Z]{2,5}\b(?=\s|,|\.|$|\))").unwrap());

static MONEY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$[\d,.]+[BMKbmk]?|\d+\.?\d*\s*%|\d+[xX]\b").unwrap());

static TIME_REFS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(today|yesterday|tomorrow|last\s+(?:week|month|quarter|year)|next\s+(?:week|month|quarter|year)|this\s+(?:week|month|quarter|year)|Q[1-4]\s*\d{2,4}|FY\s*\d{2,4}|\d{4}|\d{1,2}/\d{1,2}/\d{2,4}|since\s+\w+|before\s+\w+|after\s+\w+|YTD|MTD|QTD)\b",
    )
    .unwrap()
});

static CONVO_REFS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(what we discussed|as I said|like I mentioned|my portfolio|we talked about|you said|you mentioned|earlier|previously|last time|before this|our conversation|what you told me|remember when|as we agreed|my account|my positions?)\b",
    )
    .unwrap()
});

static NEGATIONS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(don'?t|do not|not|never|no|exclude|ignore|without|isn'?t|aren'?t|won'?t|can'?t|shouldn'?t|wouldn'?t|doesn'?t|haven'?t|hasn'?t|avoid|skip|except)\b",
    )
    .unwrap()
});

static QUESTION_ANCHORS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(who|what|when|where|why|how|which|compare|difference|versus|vs\.?|better|worse|rank|list|explain|summarize|analyze|recommend|suggest|should I|is it)\b",
    )
    .unwrap()
});

static FILLER_NEGATION_PHRASES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:not too much trouble|not a problem|no worries|not at all|no rush|not urgent|if it's not)",
    )
    .unwrap()
});

// ============================================================
//  FILLER PATTERNS
// ============================================================

struct FillerPattern {
    regex: Regex,
    replacement: &'static str,
}

static FILLER_PATTERNS: LazyLock<Vec<FillerPattern>> = LazyLock::new(|| {
    vec![
        // Hedging openers
        fp(r"(?i)\bI was wondering if (?:you could |maybe )?", ""),
        fp(r"(?i)\bcould you (?:please |maybe |possibly )?", ""),
        fp(r"(?i)\bwould you (?:be able to |mind )?", ""),
        fp(r"(?i)\bI would (?:really )?like (?:you )?to ", ""),
        fp(r"(?i)\bcan you (?:please )?", ""),
        fp(r"(?i)\bdo you think you could ", ""),
        fp(r"(?i)\bif (?:it's )?(?:not too much trouble|possible),?\s*", ""),
        // Politeness padding
        fp(r"(?i)\b(?:please|thanks|thank you|thx|pls)\b[,.]?\s*", ""),
        fp(r"(?i)^(?:hey|hi|hello|yo|sup)(?:\s+there)?[,.\s]+", ""),
        fp(r"(?i)^(?:so|ok so|okay so|alright so|okay)\s+", ""),
        // Filler words
        fp(
            r"(?i)\b(?:basically|actually|honestly|really|just|very|quite|pretty much|kind of|sort of|literally|obviously|clearly|simply|definitely|certainly|absolutely|essentially|fundamentally|in my opinion|I think that|I believe that|it seems like|to be honest|as you know|you know|I mean|if you know what I mean|at the end of the day|needless to say)\b,?\s*",
            "",
        ),
        // Redundant closers
        fp(
            r"(?i)\s*(?:thanks in advance|thank you so much|I really appreciate it|that would be great|that would be awesome|I'd appreciate it|much appreciated|cheers)\.?\s*$",
            "",
        ),
        // Whitespace cleanup
        fp(r"  +", " "),
        fp(r"\n\s*\n\s*\n+", "\n\n"),
    ]
});

fn fp(pattern: &str, replacement: &'static str) -> FillerPattern {
    FillerPattern {
        regex: Regex::new(pattern).unwrap(),
        replacement,
    }
}

// ============================================================
//  CORE FUNCTIONS
// ============================================================

pub fn extract_protected(text: &str) -> Vec<ProtectedEntity> {
    let mut protected = Vec::new();

    // Find filler negation spans to exclude
    let filler_neg_spans: Vec<(usize, usize)> = FILLER_NEGATION_PHRASES
        .find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect();

    // Standard regex patterns
    let std_patterns: &[(& Regex, &str)] = &[
        (&MONEY_PATTERN, "money"),
        (&TIME_REFS, "time_ref"),
        (&CONVO_REFS, "convo_ref"),
        (&NEGATIONS, "negation"),
        (&QUESTION_ANCHORS, "question"),
    ];

    // Ticker uses fancy-regex (lookahead)
    if let Ok(iter) = TICKER_PATTERN.find_iter(text).collect::<Result<Vec<_>, _>>() {
        for m in iter {
            protected.push(ProtectedEntity {
                text: m.as_str().to_string(),
                kind: "ticker".to_string(),
            });
        }
    }

    for (pattern, label) in std_patterns {
        for m in pattern.find_iter(text) {
            if *label == "negation" {
                let in_filler = filler_neg_spans
                    .iter()
                    .any(|&(s, e)| s <= m.start() && m.end() <= e);
                if in_filler {
                    continue;
                }
            }
            protected.push(ProtectedEntity {
                text: m.as_str().to_string(),
                kind: label.to_string(),
            });
        }
    }

    protected
}

pub fn is_code_or_json(text: &str) -> bool {
    let indicators = [
        "{", "}", "def ", "class ", "import ", "function ", "const ", "var ", "let ", "```",
        "===", "!==",
    ];
    indicators.iter().filter(|i| text.contains(**i)).count() >= 3
}

pub fn calculate_confidence(
    original_protected: &[ProtectedEntity],
    optimized_protected: &[ProtectedEntity],
) -> f64 {
    let match_rate = |orig: &[ProtectedEntity], opt: &[ProtectedEntity], label: &str| -> f64 {
        let orig_set: std::collections::HashSet<String> = orig
            .iter()
            .filter(|e| e.kind == label)
            .map(|e| e.text.to_lowercase())
            .collect();
        let opt_set: std::collections::HashSet<String> = opt
            .iter()
            .filter(|e| e.kind == label)
            .map(|e| e.text.to_lowercase())
            .collect();
        if orig_set.is_empty() {
            return 1.0;
        }
        orig_set.intersection(&opt_set).count() as f64 / orig_set.len() as f64
    };

    let critical_types = ["ticker", "money", "time_ref", "convo_ref"];
    let active_critical: Vec<f64> = critical_types
        .iter()
        .filter(|t| original_protected.iter().any(|e| e.kind == **t))
        .map(|t| match_rate(original_protected, optimized_protected, t))
        .collect();

    let entity_score = if active_critical.is_empty() {
        1.0
    } else {
        active_critical
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min)
    };

    let intent_score = match_rate(original_protected, optimized_protected, "question");
    let negation_score = match_rate(original_protected, optimized_protected, "negation");

    (entity_score * 0.4) + (intent_score * 0.3) + (negation_score * 0.3)
}

pub fn optimize_prompt(text: &str, confidence_threshold: f64, min_tokens: usize) -> OptimizationResult {
    let original_tokens = estimate_tokens(text);

    // Skip conditions
    if is_code_or_json(text) {
        return OptimizationResult {
            original: text.to_string(),
            optimized: text.to_string(),
            original_tokens,
            optimized_tokens: original_tokens,
            confidence: 1.0,
            protected_entities: vec![],
            sent_optimized: false,
            skip_reason: "code_or_json".to_string(),
        };
    }

    if original_tokens < min_tokens {
        return OptimizationResult {
            original: text.to_string(),
            optimized: text.to_string(),
            original_tokens,
            optimized_tokens: original_tokens,
            confidence: 1.0,
            protected_entities: vec![],
            sent_optimized: false,
            skip_reason: "below_min_tokens".to_string(),
        };
    }

    // Step 1: Extract protected entities
    let protected_before = extract_protected(text);

    // Step 2: Remove filler
    let mut optimized = text.to_string();
    for fp in FILLER_PATTERNS.iter() {
        optimized = fp.regex.replace_all(&optimized, fp.replacement).to_string();
    }

    // Clean up edges
    optimized = optimized.trim().to_string();
    let leading_clean = Regex::new(r"^[,.\s]+").unwrap();
    optimized = leading_clean.replace(&optimized, "").to_string();
    if let Some(first) = optimized.chars().next() {
        if first.is_lowercase() {
            let upper: String = first.to_uppercase().collect();
            optimized = format!("{}{}", upper, &optimized[first.len_utf8()..]);
        }
    }

    let optimized_tokens = estimate_tokens(&optimized);

    // Step 3: Confidence check
    let protected_after = extract_protected(&optimized);
    let confidence = calculate_confidence(&protected_before, &protected_after);

    // Step 4: Decision
    let ratio = if original_tokens > 0 {
        optimized_tokens as f64 / original_tokens as f64
    } else {
        1.0
    };
    let not_worth_it = ratio > 0.95;
    let confident_enough = confidence >= confidence_threshold;
    let sent_optimized = confident_enough && !not_worth_it;

    let skip_reason = if !confident_enough {
        format!("low_confidence ({confidence:.2})")
    } else if not_worth_it {
        format!("negligible_savings ({ratio:.2})")
    } else {
        String::new()
    };

    let entity_texts: Vec<String> = protected_before.iter().map(|e| e.text.clone()).collect();

    OptimizationResult {
        original: text.to_string(),
        optimized: if sent_optimized {
            optimized
        } else {
            text.to_string()
        },
        original_tokens,
        optimized_tokens: if sent_optimized {
            optimized_tokens
        } else {
            original_tokens
        },
        confidence,
        protected_entities: entity_texts,
        sent_optimized,
        skip_reason,
    }
}

/// Convenience wrapper with default thresholds
pub fn optimize_prompt_default(text: &str) -> OptimizationResult {
    optimize_prompt(text, 0.90, 50)
}
