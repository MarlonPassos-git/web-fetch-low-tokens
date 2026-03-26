use ego_tree::NodeId;
use regex::Regex;
use scraper::{Html, Selector};
use std::sync::LazyLock;

/// Tags that are pure noise — content inside them is removed
const REMOVE_TAGS: &[&str] = &[
    "script", "style", "nav", "footer", "header", "aside",
    "iframe", "noscript", "svg", "form", "button",
];

/// CSS classes/ids that indicate noise
static NOISE_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(\bad[\w-]*|sidebar|cookie|consent|popup|modal|newsletter|social|share|comment|related|promo|banner|sponsor|disclaimer|footer|nav|menu|breadcrumb)"
    ).unwrap()
});

static WHITESPACE_COLLAPSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

/// Strip HTML down to readable text content.
pub fn clean_html(raw_html: &str) -> String {
    let document = Html::parse_document(raw_html);

    // Step 1: Find best content container
    let container_selectors = [
        r#"div[id="mw-content-text"], div[id="main-content"]"#,
        "article",
        "main",
        r#"div[role="main"]"#,
    ];

    let mut container_selector = None;
    for sel_str in &container_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if document.select(&sel).next().is_some() {
                container_selector = Some(sel);
                break;
            }
        }
    }

    // Collect text from the container (or whole document)
    let root = if let Some(ref sel) = container_selector {
        document.select(sel).next()
    } else {
        None
    };

    // Build set of element IDs to skip (noise tags + noise classes)
    let mut skip_ids = std::collections::HashSet::new();

    // Mark noise tags for removal
    for tag_name in REMOVE_TAGS {
        if let Ok(sel) = Selector::parse(tag_name) {
            for el in document.select(&sel) {
                skip_ids.insert(el.id());
            }
        }
    }

    // Mark noisy class/id elements for removal
    if let Ok(sel) = Selector::parse("*") {
        for el in document.select(&sel) {
            let el_ref = el.value();
            let classes = el_ref.classes().collect::<Vec<_>>().join(" ");
            let id = el_ref.id().unwrap_or("");
            let combined = format!("{classes} {id}");
            if NOISE_PATTERNS.is_match(&combined) {
                skip_ids.insert(el.id());
            }
        }
    }

    // Extract text, skipping noise elements
    let mut lines = Vec::new();

    // Walk the tree collecting text nodes, skipping those under noise elements
    // Since scraper doesn't give us a simple "is descendant of" check,
    // we'll use a simpler approach: collect all text from root, then filter
    let text_nodes: Vec<String> = if let Some(root_el) = root {
        collect_text_from_element(&document, root_el, &skip_ids)
    } else {
        // Fallback: use body or whole document
        if let Ok(body_sel) = Selector::parse("body") {
            if let Some(body) = document.select(&body_sel).next() {
                collect_text_from_element(&document, body, &skip_ids)
            } else {
                collect_all_text(&document, &skip_ids)
            }
        } else {
            collect_all_text(&document, &skip_ids)
        }
    };

    for text in text_nodes {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }

    let text = lines.join("\n");
    WHITESPACE_COLLAPSE.replace_all(&text, "\n\n").to_string()
}

fn collect_text_from_element(
    _doc: &Html,
    element: scraper::ElementRef,
    skip_ids: &std::collections::HashSet<NodeId>,
) -> Vec<String> {
    let container_id = element.id();
    let mut texts = Vec::new();

    for descendant in element.traverse() {
        match descendant {
            ego_tree::iter::Edge::Open(node_ref) => {
                if let Some(el) = scraper::ElementRef::wrap(node_ref) {
                    if skip_ids.contains(&el.id()) {
                        continue;
                    }
                }
                if let Some(text) = node_ref.value().as_text() {
                    // Walk up ancestors but stop at the container to avoid
                    // false positives from noise elements above (e.g. html, body).
                    let mut should_skip = false;
                    let mut current = node_ref.parent();
                    while let Some(parent) = current {
                        if let Some(parent_el) = scraper::ElementRef::wrap(parent) {
                            if parent_el.id() == container_id {
                                break; // reached container — stop, don't go higher
                            }
                            if skip_ids.contains(&parent_el.id()) {
                                should_skip = true;
                                break;
                            }
                        }
                        current = parent.parent();
                    }
                    if !should_skip {
                        texts.push(text.to_string());
                    }
                }
            }
            ego_tree::iter::Edge::Close(_) => {}
        }
    }

    texts
}

fn collect_all_text(
    doc: &Html,
    skip_ids: &std::collections::HashSet<NodeId>,
) -> Vec<String> {
    if let Ok(body_sel) = Selector::parse("body") {
        if let Some(body) = doc.select(&body_sel).next() {
            return collect_text_from_element(doc, body, skip_ids);
        }
    }
    // Last resort: traverse all nodes from root
    let mut texts = Vec::new();
    for edge in doc.tree.root().traverse() {
        if let ego_tree::iter::Edge::Open(node_ref) = edge {
            if let Some(text) = node_ref.value().as_text() {
                texts.push(text.to_string());
            }
        }
    }
    texts
}
