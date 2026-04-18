#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Local;
use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[path = "../paths.rs"]
mod paths;
#[path = "../search_index/mod.rs"]
mod search_index;
#[path = "../session_manager/mod.rs"]
mod session_manager;

const DEFAULT_WARMUP_ITERATIONS: usize = 3;
const DEFAULT_MEASURED_ITERATIONS: usize = 20;
const DEFAULT_RESULT_LIMIT: usize = 20;

fn main() {
    if let Err(err) = run() {
        eprintln!("search-benchmark failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = BenchmarkArgs::parse(env::args().skip(1).collect())?;
    let snapshot = prepare_benchmark_snapshot()?;
    let status = search_index::get_index_status()?;
    if !status.ready || status.messages_count <= 0 {
        return Err(format!(
            "Search index is not ready. DB: {}. Run rebuild_search_index() first.",
            status.db_path
        ));
    }

    let output_path = args
        .output_path
        .clone()
        .unwrap_or_else(default_output_path);
    let queries = resolve_queries(&args, &status.db_path)?;
    if queries.is_empty() {
        return Err("No benchmark queries available. Provide --query or ensure the index has searchable messages.".to_string());
    }

    let query_logs = queries
        .iter()
        .map(|query| benchmark_query(query, &args))
        .collect::<Result<Vec<_>, _>>()?;

    let benchmark_log = BenchmarkLog {
        generated_at: Local::now().to_rfc3339(),
        tool: ToolInfo {
            name: "search-benchmark".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        source_db_path: snapshot.source_db_path.display().to_string(),
        source_db_size_bytes: snapshot.source_db_size_bytes,
        config: BenchmarkConfig {
            warmup_iterations: args.warmup_iterations,
            measured_iterations: args.measured_iterations,
            result_limit: args.result_limit,
            sort_by: args.sort_by.clone(),
        },
        index_status: status,
        sampled_queries: query_logs,
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create benchmark log dir {}: {e}", parent.display()))?;
    }

    let serialized = serde_json::to_string_pretty(&benchmark_log)
        .map_err(|e| format!("Failed to serialize benchmark log: {e}"))?;
    fs::write(&output_path, serialized)
        .map_err(|e| format!("Failed to write benchmark log {}: {e}", output_path.display()))?;

    println!("Search benchmark baseline written to {}", output_path.display());
    println!(
        "Indexed sessions: {}, indexed messages: {}, db size: {} bytes",
        benchmark_log.index_status.sessions_count,
        benchmark_log.index_status.messages_count,
        benchmark_log.source_db_size_bytes
    );
    for query in &benchmark_log.sampled_queries {
        println!(
            "[{}] query=\"{}\" hits={} avg={:.3}ms p95={:.3}ms min={:.3}ms max={:.3}ms",
            query.label,
            query.query,
            query.total_count,
            query.avg_ms,
            query.p95_ms,
            query.min_ms,
            query.max_ms
        );
    }

    Ok(())
}

fn prepare_benchmark_snapshot() -> Result<BenchmarkSnapshotGuard, String> {
    let source_db_path = paths::get_search_db_path();
    if !source_db_path.exists() {
        return Err(format!(
            "Search DB not found at {}. Build the current search index first.",
            source_db_path.display()
        ));
    }

    let snapshot_dir = env::temp_dir().join(format!(
        "acliv-search-benchmark-{}-{}",
        std::process::id(),
        Local::now().format("%Y%m%d%H%M%S")
    ));
    fs::create_dir_all(&snapshot_dir).map_err(|e| {
        format!(
            "Failed to create benchmark snapshot dir {}: {e}",
            snapshot_dir.display()
        )
    })?;

    copy_if_exists(&source_db_path, &snapshot_dir.join("search.db"))?;
    copy_if_exists(
        &source_db_path.with_file_name("search.db-wal"),
        &snapshot_dir.join("search.db-wal"),
    )?;
    copy_if_exists(
        &source_db_path.with_file_name("search.db-shm"),
        &snapshot_dir.join("search.db-shm"),
    )?;

    env::set_var("ACLIV_INDEX_DIR", &snapshot_dir);

    let source_db_size_bytes = fs::metadata(&source_db_path)
        .ok()
        .and_then(|metadata| i64::try_from(metadata.len()).ok())
        .unwrap_or(0);

    Ok(BenchmarkSnapshotGuard {
        source_db_path,
        source_db_size_bytes,
        snapshot_dir,
    })
}

fn copy_if_exists(from: &Path, to: &Path) -> Result<(), String> {
    if !from.exists() {
        return Ok(());
    }

    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("Failed to copy {} to {}: {e}", from.display(), to.display()))
}

fn default_output_path() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap_or(manifest_dir);
    let file_name = format!(
        "search-benchmark-baseline-{}.log",
        Local::now().format("%Y%m%d-%H%M%S")
    );
    repo_root.join(".tmp").join(file_name)
}

fn resolve_queries(args: &BenchmarkArgs, db_path: &str) -> Result<Vec<QueryDefinition>, String> {
    if !args.query_specs.is_empty() {
        return args
            .query_specs
            .iter()
            .enumerate()
            .map(|(index, value)| parse_query_spec(value, index))
            .collect();
    }

    if let Some(path) = args.queries_from.as_ref() {
        return load_queries_from_log(path);
    }

    sample_queries(db_path)
}

fn parse_query_spec(value: &str, index: usize) -> Result<QueryDefinition, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Empty --query value is not allowed".to_string());
    }

    let (label, query) = if let Some((label, query)) = trimmed.split_once('=') {
        (label.trim().to_string(), query.trim().to_string())
    } else {
        (format!("query-{}", index + 1), trimmed.to_string())
    };

    if label.is_empty() || query.is_empty() {
        return Err(format!("Invalid --query value: {trimmed}"));
    }

    Ok(QueryDefinition {
        label,
        source: "cli".to_string(),
        query,
    })
}

fn load_queries_from_log(path: &Path) -> Result<Vec<QueryDefinition>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read benchmark log {}: {e}", path.display()))?;
    let previous: ReusableBenchmarkLog = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse benchmark log {}: {e}", path.display()))?;
    if previous.sampled_queries.is_empty() {
        return Err(format!(
            "Benchmark log {} does not contain sampled queries",
            path.display()
        ));
    }
    Ok(previous
        .sampled_queries
        .into_iter()
        .map(|query| QueryDefinition {
            label: query.label,
            source: format!("reused-from:{}", path.display()),
            query: query.query,
        })
        .collect())
}

fn sample_queries(db_path: &str) -> Result<Vec<QueryDefinition>, String> {
    let connection = Connection::open(db_path)
        .map_err(|e| format!("Failed to open search DB {} for sampling: {e}", db_path))?;
    let mut stmt = connection
        .prepare(
            r#"
            SELECT content_text
            FROM messages
            WHERE COALESCE(is_sidechain, 0) = 0
              AND COALESCE(kind, 'message') = 'message'
              AND LENGTH(TRIM(content_text)) > 0
            ORDER BY COALESCE(ts, 0) DESC, id DESC
            LIMIT 1000
            "#,
        )
        .map_err(|e| format!("Failed to prepare message sampling query: {e}"))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("Failed to sample message texts: {e}"))?;

    let ascii_token_re =
        Regex::new(r"[A-Za-z][A-Za-z0-9_]{5,31}").map_err(|e| format!("ASCII regex error: {e}"))?;
    let cjk_phrase_re =
        Regex::new(r"\p{Han}{2,8}").map_err(|e| format!("CJK regex error: {e}"))?;

    let mut english_hit: Option<QueryDefinition> = None;
    let mut cjk_hit: Option<QueryDefinition> = None;

    for row in rows {
        let content = row.map_err(|e| format!("Failed to read sampled message text: {e}"))?;
        if english_hit.is_none() {
            if let Some(token) = pick_ascii_token(&content, &ascii_token_re) {
                english_hit = Some(QueryDefinition {
                    label: "english-hit".to_string(),
                    source: "sampled-recent-message".to_string(),
                    query: token,
                });
            }
        }
        if cjk_hit.is_none() {
            if let Some(phrase) = pick_cjk_phrase(&content, &cjk_phrase_re) {
                cjk_hit = Some(QueryDefinition {
                    label: "cjk-hit".to_string(),
                    source: "sampled-recent-message".to_string(),
                    query: phrase,
                });
            }
        }

        if english_hit.is_some() && cjk_hit.is_some() {
            break;
        }
    }

    let mut queries = Vec::new();
    if let Some(query) = english_hit {
        queries.push(query);
    }
    if let Some(query) = cjk_hit {
        queries.push(query);
    }
    queries.push(QueryDefinition {
        label: "english-miss".to_string(),
        source: "synthetic-miss".to_string(),
        query: format!("ACLIV_BENCH_MISS_TOKEN_{}", Local::now().format("%Y%m%d%H%M%S")),
    });
    queries.push(QueryDefinition {
        label: "cjk-miss".to_string(),
        source: "synthetic-miss".to_string(),
        query: format!("基线对比未命中样本{}", Local::now().format("%H%M%S")),
    });

    Ok(queries)
}

fn pick_ascii_token(content: &str, pattern: &Regex) -> Option<String> {
    let mut fallback = None;
    for candidate in pattern.find_iter(content).map(|value| value.as_str()) {
        if candidate.len() < 6 {
            continue;
        }
        if is_distinctive_ascii_token(candidate) {
            return Some(candidate.to_string());
        }
        if fallback.is_none() {
            fallback = Some(candidate.to_string());
        }
    }
    fallback
}

fn is_distinctive_ascii_token(candidate: &str) -> bool {
    candidate.contains('_')
        || candidate.chars().any(|ch| ch.is_ascii_digit())
        || candidate.chars().any(|ch| ch.is_ascii_uppercase())
}

fn pick_cjk_phrase(content: &str, pattern: &Regex) -> Option<String> {
    let matched = pattern.find(content)?.as_str();
    let chars = matched.chars().collect::<Vec<_>>();
    if chars.len() <= 4 {
        return Some(matched.to_string());
    }

    let target_len = if chars.len() >= 4 { 4 } else { 2 };
    Some(chars.into_iter().take(target_len).collect())
}

fn benchmark_query(query: &QueryDefinition, args: &BenchmarkArgs) -> Result<QueryBenchmark, String> {
    for _ in 0..args.warmup_iterations {
        let _ = search_index::search_content(
            &query.query,
            args.result_limit,
            None,
            None,
            None,
            Some(args.sort_by.as_str()),
        )?;
    }

    let mut durations_ms = Vec::with_capacity(args.measured_iterations);
    let mut total_count = None;
    let mut returned_hits = None;

    for _ in 0..args.measured_iterations {
        let started = Instant::now();
        let result = search_index::search_content(
            &query.query,
            args.result_limit,
            None,
            None,
            None,
            Some(args.sort_by.as_str()),
        )?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        durations_ms.push(elapsed_ms);

        total_count.get_or_insert(result.total_count);
        returned_hits.get_or_insert(result.hits.len());
    }

    let stats = compute_stats(&durations_ms)?;

    Ok(QueryBenchmark {
        label: query.label.clone(),
        source: query.source.clone(),
        query: query.query.clone(),
        has_cjk: contains_cjk(&query.query),
        total_count: total_count.unwrap_or_default(),
        returned_hits: returned_hits.unwrap_or_default(),
        min_ms: stats.min_ms,
        max_ms: stats.max_ms,
        avg_ms: stats.avg_ms,
        median_ms: stats.median_ms,
        p95_ms: stats.p95_ms,
        durations_ms,
    })
}

fn compute_stats(durations_ms: &[f64]) -> Result<QueryStats, String> {
    if durations_ms.is_empty() {
        return Err("No duration samples available".to_string());
    }

    let min_ms = durations_ms
        .iter()
        .copied()
        .reduce(f64::min)
        .ok_or_else(|| "Failed to compute min duration".to_string())?;
    let max_ms = durations_ms
        .iter()
        .copied()
        .reduce(f64::max)
        .ok_or_else(|| "Failed to compute max duration".to_string())?;
    let avg_ms = durations_ms.iter().sum::<f64>() / durations_ms.len() as f64;

    let mut sorted = durations_ms.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let median_ms = percentile(&sorted, 0.5);
    let p95_ms = percentile(&sorted, 0.95);

    Ok(QueryStats {
        min_ms,
        max_ms,
        avg_ms,
        median_ms,
        p95_ms,
    })
}

fn percentile(sorted_samples: &[f64], percentile: f64) -> f64 {
    if sorted_samples.len() == 1 {
        return sorted_samples[0];
    }

    let clamped = percentile.clamp(0.0, 1.0);
    let rank = ((sorted_samples.len() - 1) as f64 * clamped).round() as usize;
    sorted_samples[rank]
}

fn contains_cjk(input: &str) -> bool {
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

#[derive(Debug)]
struct BenchmarkArgs {
    output_path: Option<PathBuf>,
    queries_from: Option<PathBuf>,
    query_specs: Vec<String>,
    warmup_iterations: usize,
    measured_iterations: usize,
    result_limit: usize,
    sort_by: String,
}

impl BenchmarkArgs {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut parsed = Self {
            output_path: None,
            queries_from: None,
            query_specs: Vec::new(),
            warmup_iterations: DEFAULT_WARMUP_ITERATIONS,
            measured_iterations: DEFAULT_MEASURED_ITERATIONS,
            result_limit: DEFAULT_RESULT_LIMIT,
            sort_by: "relevance".to_string(),
        };

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--output" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "Missing value for --output".to_string())?;
                    parsed.output_path = Some(PathBuf::from(value));
                }
                "--queries-from" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "Missing value for --queries-from".to_string())?;
                    parsed.queries_from = Some(PathBuf::from(value));
                }
                "--query" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "Missing value for --query".to_string())?;
                    parsed.query_specs.push(value.clone());
                }
                "--warmup" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "Missing value for --warmup".to_string())?;
                    parsed.warmup_iterations = value
                        .parse::<usize>()
                        .map_err(|e| format!("Invalid --warmup value {value}: {e}"))?;
                }
                "--iterations" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "Missing value for --iterations".to_string())?;
                    parsed.measured_iterations = value
                        .parse::<usize>()
                        .map_err(|e| format!("Invalid --iterations value {value}: {e}"))?;
                }
                "--limit" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "Missing value for --limit".to_string())?;
                    parsed.result_limit = value
                        .parse::<usize>()
                        .map_err(|e| format!("Invalid --limit value {value}: {e}"))?;
                }
                "--sort-by" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "Missing value for --sort-by".to_string())?;
                    parsed.sort_by = value.clone();
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => {
                    return Err(format!("Unknown argument: {other}"));
                }
            }
            index += 1;
        }

        if parsed.warmup_iterations == 0 {
            return Err("--warmup must be greater than 0".to_string());
        }
        if parsed.measured_iterations == 0 {
            return Err("--iterations must be greater than 0".to_string());
        }
        if parsed.result_limit == 0 {
            return Err("--limit must be greater than 0".to_string());
        }

        Ok(parsed)
    }
}

fn print_help() {
    println!("Usage: cargo run --no-default-features --bin search-benchmark -- [options]");
    println!("  --output <path>         Write log to a custom path");
    println!("  --queries-from <path>   Reuse sampled queries from an existing benchmark log");
    println!("  --query <label=query>   Add a custom benchmark query; may be repeated");
    println!("  --warmup <n>            Warmup iterations per query (default: {DEFAULT_WARMUP_ITERATIONS})");
    println!("  --iterations <n>        Measured iterations per query (default: {DEFAULT_MEASURED_ITERATIONS})");
    println!("  --limit <n>             Search result limit per query (default: {DEFAULT_RESULT_LIMIT})");
    println!("  --sort-by <mode>        Search sort mode: relevance or recent (default: relevance)");
}

#[derive(Debug, Clone)]
struct QueryDefinition {
    label: String,
    source: String,
    query: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkLog {
    generated_at: String,
    tool: ToolInfo,
    source_db_path: String,
    source_db_size_bytes: i64,
    config: BenchmarkConfig,
    index_status: search_index::SearchIndexStatus,
    sampled_queries: Vec<QueryBenchmark>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReusableBenchmarkLog {
    sampled_queries: Vec<QueryBenchmark>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolInfo {
    name: String,
    version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkConfig {
    warmup_iterations: usize,
    measured_iterations: usize,
    result_limit: usize,
    sort_by: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryBenchmark {
    label: String,
    source: String,
    query: String,
    has_cjk: bool,
    total_count: i64,
    returned_hits: usize,
    min_ms: f64,
    max_ms: f64,
    avg_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    durations_ms: Vec<f64>,
}

#[derive(Debug)]
struct QueryStats {
    min_ms: f64,
    max_ms: f64,
    avg_ms: f64,
    median_ms: f64,
    p95_ms: f64,
}

struct BenchmarkSnapshotGuard {
    source_db_path: PathBuf,
    source_db_size_bytes: i64,
    snapshot_dir: PathBuf,
}

impl Drop for BenchmarkSnapshotGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.snapshot_dir);
    }
}
