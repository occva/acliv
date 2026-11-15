use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use chrono::{DateTime, FixedOffset};
use regex::Regex;
use serde_json::Value;

static FILE_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(file://[^)]+\)").expect("valid file link regex"));

/// Read the first `head_n` lines and last `tail_n` lines from a file.
/// For small files (< 16 KB), reads all lines once to avoid unnecessary seeking.
pub fn read_head_tail_lines(
    path: &Path,
    head_n: usize,
    tail_n: usize,
) -> io::Result<(Vec<String>, Vec<String>)> {
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();

    // For small files, read all lines once and split
    if file_len < 16_384 {
        let reader = BufReader::new(file);
        let all: Vec<String> = reader.lines().map_while(Result::ok).collect();
        let head = all.iter().take(head_n).cloned().collect();
        let skip = all.len().saturating_sub(tail_n);
        let tail = all.into_iter().skip(skip).collect();
        return Ok((head, tail));
    }

    // Read head lines from the beginning
    let reader = BufReader::new(file);
    let head: Vec<String> = reader.lines().take(head_n).map_while(Result::ok).collect();

    // Seek to last ~16 KB for tail lines
    let seek_pos = file_len.saturating_sub(16_384);
    let mut file2 = File::open(path)?;
    file2.seek(SeekFrom::Start(seek_pos))?;
    let tail_reader = BufReader::new(file2);
    let all_tail: Vec<String> = tail_reader.lines().map_while(Result::ok).collect();

    // Skip first partial line if we seeked into the middle of a line
    let skip_first = if seek_pos > 0 { 1 } else { 0 };
    let usable: Vec<String> = all_tail.into_iter().skip(skip_first).collect();
    let skip = usable.len().saturating_sub(tail_n);
    let tail = usable.into_iter().skip(skip).collect();

    Ok((head, tail))
}

pub fn parse_timestamp_to_ms(value: &Value) -> Option<i64> {
    let raw = value.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt: DateTime<FixedOffset>| dt.timestamp_millis())
}

pub fn extract_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.to_string(),
        Value::Array(items) => items
            .iter()
            .filter_map(extract_text_from_item)
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn extract_text_from_item(item: &Value) -> Option<String> {
    if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }

    if let Some(text) = item.get("input_text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }

    if let Some(text) = item.get("output_text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }

    if let Some(content) = item.get("content") {
        let text = extract_text(content);
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

pub fn truncate_summary(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut result = trimmed.chars().take(max_chars).collect::<String>();
    result.push_str("...");
    result
}

pub fn normalize_title_candidate(text: &str, max_chars: usize) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_file_links = FILE_LINK_RE.replace_all(trimmed, "$1");
    let decoded = decode_percent_escapes(without_file_links.as_ref());
    let normalized = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }

    Some(truncate_summary(&normalized, max_chars))
}

fn decode_percent_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_value(bytes[i + 1]);
            let lo = hex_value(bytes[i + 2]);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                output.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }

        output.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn path_basename(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.trim_end_matches(['/', '\\']);
    let last = normalized
        .split(['/', '\\'])
        .next_back()
        .filter(|segment| !segment.is_empty())?;
    Some(last.to_string())
}

pub fn log_scan_error(provider: &str, path: &Path, err: &io::Error) {
    eprintln!(
        "[WARN] provider={provider} failed to read {}: {}",
        path.display(),
        err
    );
}

/// SQLite writers commonly leave the newest state in `<db>-wal` until checkpoint.
/// The source fingerprint therefore covers both the main database and its WAL.
pub fn sqlite_source_state(path: &Path) -> (Option<i64>, Option<i64>) {
    let mut wal_name = path.as_os_str().to_os_string();
    wal_name.push("-wal");
    let candidates = [path.to_path_buf(), PathBuf::from(wal_name)];
    let mut latest_mtime = None;
    let mut total_size = 0_i64;
    let mut found = false;

    for candidate in candidates {
        let Ok(metadata) = fs::metadata(candidate) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        found = true;
        if let Some(mtime) = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .and_then(|value| i64::try_from(value.as_millis()).ok())
        {
            latest_mtime = Some(latest_mtime.map_or(mtime, |current: i64| current.max(mtime)));
        }
        total_size = total_size.saturating_add(i64::try_from(metadata.len()).unwrap_or(i64::MAX));
    }

    (latest_mtime, found.then_some(total_size))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{normalize_title_candidate, sqlite_source_state};

    #[test]
    fn normalize_title_candidate_decodes_file_links_and_percent_sequences() {
        let title = normalize_title_candidate(
            "[@%E7%A4%BE%E5%9B%A2%E9%A6%96%E9%A1%B5.htm](file:///D:/code/demo-script/web-tools/%E7%A4%BE%E5%9B%A2%E9%A6%96%E9%A1%B5.htm)",
            160,
        );

        assert_eq!(title.as_deref(), Some("@社团首页.htm"));
    }

    #[test]
    fn sqlite_source_state_includes_wal_bytes() {
        let dir = tempdir().expect("tempdir");
        let database = dir.path().join("source.db");
        let wal = dir.path().join("source.db-wal");
        fs::write(&database, b"db").expect("write database");
        fs::write(&wal, b"wal!").expect("write wal");

        let (mtime, size) = sqlite_source_state(&database);
        assert!(mtime.is_some());
        assert_eq!(size, Some(6));
    }
}
