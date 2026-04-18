use std::collections::HashSet;
use std::sync::OnceLock;

use jieba_rs::Jieba;

pub fn contains_cjk(input: &str) -> bool {
    input.chars().any(|ch| {
        matches!(
            ch as u32,
            0x3400..=0x4DBF
                | 0x4E00..=0x9FFF
                | 0xF900..=0xFAFF
                | 0x20000..=0x2A6DF
                | 0x2A700..=0x2B73F
                | 0x2B740..=0x2B81F
                | 0x2B820..=0x2CEAF
                | 0x2CEB0..=0x2EBEF
        )
    })
}

pub fn normalize_search_text(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if !contains_cjk(trimmed) {
        return trimmed.to_string();
    }

    let tokens = tokenize_terms(trimmed);
    if tokens.is_empty() {
        trimmed.to_string()
    } else {
        tokens.join(" ")
    }
}

pub fn build_fts_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if uses_fts_syntax(trimmed) {
        return trimmed.to_string();
    }

    if !contains_cjk(trimmed) {
        return format!("\"{}\"", trimmed.replace('"', "\"\""));
    }

    let tokens = tokenize_terms(trimmed);
    if tokens.is_empty() {
        return format!("\"{}\"", trimmed.replace('"', "\"\""));
    }

    tokens
        .into_iter()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn should_run_substring_fallback(query: &str, fts_total_count: i64) -> bool {
    if fts_total_count > 0 {
        return false;
    }

    let trimmed = query.trim();
    if trimmed.is_empty() {
        return false;
    }

    if uses_fts_syntax(trimmed) {
        return false;
    }

    if contains_cjk(trimmed) {
        let cjk_chars = trimmed.chars().filter(|ch| contains_cjk_char(*ch)).count();
        return cjk_chars <= 4;
    }

    !is_plain_token_query(trimmed)
}

pub fn build_highlight_snippet(content: &str, query: &str) -> String {
    for candidate in snippet_candidates(query) {
        if let Some(snippet) = build_snippet_for_term(content, &candidate) {
            return snippet;
        }
    }

    build_prefix_snippet(content)
}

pub fn best_snippet_probe(query: &str) -> Option<String> {
    let candidates = snippet_candidates(query);
    candidates
        .iter()
        .find(|candidate| !candidate.chars().any(char::is_whitespace))
        .cloned()
        .or_else(|| candidates.first().cloned())
}

fn tokenize_terms(input: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tokens = Vec::new();
    for token in jieba().cut_for_search(input, true) {
        let normalized = token.trim();
        if normalized.is_empty() {
            continue;
        }
        if !normalized
            .chars()
            .any(|ch| ch.is_alphanumeric() || contains_cjk_char(ch))
        {
            continue;
        }
        if seen.insert(normalized.to_string()) {
            tokens.push(normalized.to_string());
        }
    }
    tokens
}

fn snippet_candidates(query: &str) -> Vec<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    if !uses_fts_syntax(trimmed) {
        let raw = trimmed.to_string();
        if seen.insert(raw.clone()) {
            candidates.push(raw);
        }
    }

    if let Some(literal_query) = literal_query_candidate(trimmed) {
        if seen.insert(literal_query.clone()) {
            candidates.push(literal_query);
        }
    }

    let mut token_candidates = tokenize_query_terms(trimmed);
    token_candidates.sort_by(|left, right| {
        right
            .chars()
            .count()
            .cmp(&left.chars().count())
            .then_with(|| left.cmp(right))
    });
    for token in token_candidates {
        if seen.insert(token.clone()) {
            candidates.push(token);
        }
    }

    candidates
}

fn build_snippet_for_term(content: &str, needle: &str) -> Option<String> {
    let chars = content.chars().collect::<Vec<_>>();
    if chars.is_empty() || needle.is_empty() {
        return None;
    }

    let (match_char_index, query_len) = find_match_char_range(content, needle)?;
    let start = match_char_index.saturating_sub(20);
    let end = (match_char_index + query_len + 20).min(chars.len());

    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < chars.len() { "…" } else { "" };

    let before = chars[start..match_char_index].iter().collect::<String>();
    let matched = chars[match_char_index..(match_char_index + query_len).min(chars.len())]
        .iter()
        .collect::<String>();
    let after = chars[(match_char_index + query_len).min(chars.len())..end]
        .iter()
        .collect::<String>();

    Some(format!(
        "{}{}<mark>{}</mark>{}{}",
        prefix,
        escape_html(&before),
        escape_html(&matched),
        escape_html(&after),
        suffix
    ))
}

fn find_match_char_range(content: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }

    if let Some(byte_index) = content.find(needle) {
        let end_byte = byte_index + needle.len();
        let start = content[..byte_index].chars().count();
        let len = content[byte_index..end_byte].chars().count().max(1);
        return Some((start, len));
    }

    if needle.is_ascii() {
        let folded_content = content.to_ascii_lowercase();
        let folded_needle = needle.to_ascii_lowercase();
        let byte_index = folded_content.find(&folded_needle)?;
        let end_byte = byte_index + needle.len();
        let start = content[..byte_index].chars().count();
        let len = content[byte_index..end_byte].chars().count().max(1);
        return Some((start, len));
    }

    None
}

fn build_prefix_snippet(content: &str) -> String {
    let chars = content.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return String::new();
    }

    let end = chars.len().min(40);
    let suffix = if end < chars.len() { "…" } else { "" };
    let preview = chars[..end].iter().collect::<String>();
    format!("{}{}", escape_html(&preview), suffix)
}

fn is_plain_token_query(query: &str) -> bool {
    query
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '#'))
}

fn uses_fts_syntax(query: &str) -> bool {
    query.contains('"')
        || query.contains('*')
        || query.contains('(')
        || query.contains(')')
        || query
            .split_whitespace()
            .any(|token| matches!(token, "OR" | "AND" | "NOT"))
}

fn literal_query_candidate(query: &str) -> Option<String> {
    let normalized = query
        .chars()
        .map(|ch| match ch {
            '"' | '(' | ')' => ' ',
            '*' => ' ',
            _ => ch,
        })
        .collect::<String>();
    let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed
        .split_whitespace()
        .all(|token| is_fts_operator_token(token))
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn tokenize_query_terms(query: &str) -> Vec<String> {
    let normalized = query
        .chars()
        .map(|ch| match ch {
            '"' | '(' | ')' => ' ',
            '*' => ' ',
            _ => ch,
        })
        .collect::<String>();

    tokenize_terms(&normalized)
        .into_iter()
        .filter(|token| !is_fts_operator_token(token))
        .collect()
}

fn is_fts_operator_token(token: &str) -> bool {
    matches!(token.to_ascii_uppercase().as_str(), "OR" | "AND" | "NOT")
}

fn contains_cjk_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
            | 0x2CEB0..=0x2EBEF
    )
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn jieba() -> &'static Jieba {
    static JIEBA: OnceLock<Jieba> = OnceLock::new();
    JIEBA.get_or_init(Jieba::new)
}

#[cfg(test)]
mod tests {
    use super::{
        best_snippet_probe, build_fts_query, build_highlight_snippet, normalize_search_text,
        should_run_substring_fallback,
    };

    #[test]
    fn normalize_search_text_segments_cjk() {
        let normalized = normalize_search_text("请删除旧逻辑");
        assert!(normalized.contains("删除"));
        assert!(normalized.contains("旧逻辑") || normalized.contains("逻辑"));
    }

    #[test]
    fn build_fts_query_splits_cjk_tokens() {
        let query = build_fts_query("请删除旧逻辑");
        assert!(query.contains("\"删除\""));
    }

    #[test]
    fn build_highlight_snippet_uses_token_when_raw_query_missing() {
        let snippet = build_highlight_snippet("请删除旧逻辑", "删除 逻辑");
        assert!(snippet.contains("<mark>删除</mark>") || snippet.contains("<mark>逻辑</mark>"));
    }

    #[test]
    fn build_fts_query_preserves_explicit_syntax_for_cjk() {
        assert_eq!(build_fts_query("删除 OR 新建"), "删除 OR 新建");
    }

    #[test]
    fn build_fts_query_treats_lowercase_operator_words_as_plain_text() {
        assert_eq!(build_fts_query("error or warning"), "\"error or warning\"");
        assert_eq!(build_fts_query("or"), "\"or\"");
    }

    #[test]
    fn build_highlight_snippet_matches_ascii_case_insensitively() {
        let snippet = build_highlight_snippet("React hooks are useful", "react");
        assert!(snippet.contains("<mark>React</mark>"));
    }

    #[test]
    fn best_snippet_probe_prefers_real_term_over_operator() {
        assert_eq!(best_snippet_probe("删除 OR 新建").as_deref(), Some("删除"));
    }

    #[test]
    fn syntax_query_skips_substring_fallback() {
        assert!(!should_run_substring_fallback("删除 OR 新建", 0));
    }
}
