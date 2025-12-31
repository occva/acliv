// ===============================
// Data Loader - 数据加载器
// ===============================

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::models::*;

/// 全局数据缓存
pub static DATA_CACHE: std::sync::LazyLock<Arc<RwLock<HashMap<String, LoadedData>>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

/// 加载后的数据结构
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct LoadedData {
    pub projects: HashMap<String, Vec<Conversation>>,
    pub skipped_files: Vec<SkippedFile>,
    pub source: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SkippedFile {
    pub file: String,
    pub project: String,
    pub reason: String,
}

/// 获取数据源配置
pub fn get_source_config(source: &str) -> Option<SourceConfig> {
    let home = dirs::home_dir()?;
    
    match source {
        "claude" => Some(SourceConfig {
            base_dir: home.join(".claude").to_string_lossy().to_string(),
            projects_subdir: Some("projects".to_string()),
            transcripts_subdir: Some("transcripts".to_string()),
            sessions_subdir: None,
            tmp_subdir: None,
        }),
        "codex" => Some(SourceConfig {
            base_dir: home.join(".codex").to_string_lossy().to_string(),
            projects_subdir: None,
            transcripts_subdir: None,
            sessions_subdir: Some("sessions".to_string()),
            tmp_subdir: None,
        }),
        "gemini" => Some(SourceConfig {
            base_dir: home.join(".gemini").to_string_lossy().to_string(),
            projects_subdir: None,
            transcripts_subdir: None,
            sessions_subdir: None,
            tmp_subdir: Some("tmp".to_string()),
        }),
        _ => None,
    }
}

/// 列出所有支持的数据源
pub fn list_sources() -> Vec<&'static str> {
    vec!["claude", "codex", "gemini"]
}

// ==================== 工具函数 ====================

/// 解码项目名称 (如 d-code-demo -> d:\code\demo)
/// 安全提醒：由于该名称用于显示，此处需防止恶意路径构造
pub fn decode_project_name(encoded: &str) -> String {
    // 基础过滤：禁止直接的相对路径跳转字符
    let safe_encoded = encoded.replace("..", "__");
    
    let chars: Vec<char> = safe_encoded.chars().collect();
    if chars.len() >= 2 && chars[1] == '-' && chars[0].is_alphabetic() {
        let parts: Vec<&str> = safe_encoded.splitn(2, '-').collect();
        if parts.len() == 2 {
            let path_part = parts[1].replace('-', std::path::MAIN_SEPARATOR_STR);
            return format!("{}:{}{}", parts[0], std::path::MAIN_SEPARATOR, path_part);
        }
    }
    safe_encoded.replace('-', std::path::MAIN_SEPARATOR_STR)
}

/// 从 JSONL 文件加载所有行
fn load_jsonl(filepath: &Path) -> Vec<serde_json::Value> {
    const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50MB 限制防止 OOM
    
    let metadata = match fs::metadata(filepath) {
        Ok(m) => m,
        Err(e) => {
            log::error!("Failed to get metadata for {:?}: {}", filepath, e);
            return Vec::new();
        }
    };
    
    if metadata.len() > MAX_FILE_SIZE {
        log::warn!("File too large, skipping: {:?} ({} bytes)", filepath, metadata.len());
        return Vec::new();
    }

    let file = match File::open(filepath) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Failed to open file {:?}: {}", filepath, e);
            return Vec::new();
        }
    };
    
    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(|line| {
            match line {
                Ok(l) => Some(l),
                Err(e) => {
                    log::debug!("Error reading line in {:?}: {}", filepath, e);
                    None
                }
            }
        })
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            serde_json::from_str(&line).map_err(|e| {
                log::trace!("JSON parse error in {:?}: {}", filepath, e);
                e
            }).ok()
        })
        .collect()
}

/// 从 JSON 文件加载
fn load_json(filepath: &Path) -> Option<serde_json::Value> {
    let content = fs::read_to_string(filepath).ok()?;
    serde_json::from_str(&content).ok()
}

/// 从复杂内容结构中提取纯文本
pub fn extract_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            arr.iter()
                .filter_map(|item| {
                    if let serde_json::Value::String(s) = item {
                        Some(s.clone())
                    } else if let serde_json::Value::Object(obj) = item {
                        // 尝试获取 text 或 content 字段
                        obj.get("text")
                            .or_else(|| obj.get("content"))
                            .map(|v| extract_text(v))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => String::new(),
    }
}

// ==================== 消息解析器 ====================

/// 解析 Claude 格式的条目
fn parse_claude_entry(data: &serde_json::Value) -> Option<(String, String, String)> {
    let obj = data.as_object()?;
    
    let entry_type = obj.get("type")?.as_str()?;
    if entry_type != "user" && entry_type != "assistant" {
        return None;
    }
    
    // 跳过元数据和错误消息
    if obj.get("isMeta").and_then(|v| v.as_bool()).unwrap_or(false) {
        return None;
    }
    if obj.get("isApiErrorMessage").and_then(|v| v.as_bool()).unwrap_or(false) {
        return None;
    }
    
    let timestamp = obj.get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    
    let message = obj.get("message")?;
    let content_value = message.get("content")?;
    
    let content = extract_text(content_value);
    
    // 跳过以 < 开头的用户消息（通常是系统指令）
    if entry_type == "user" && content.starts_with('<') {
        return None;
    }
    
    if content.is_empty() {
        return None;
    }
    
    Some((entry_type.to_string(), content, timestamp))
}

/// 解析 Codex 格式的条目
fn parse_codex_entry(data: &serde_json::Value) -> Option<(String, String, String)> {
    let obj = data.as_object()?;
    
    let entry_type = obj.get("type")?.as_str()?;
    if entry_type != "response_item" {
        return None;
    }
    
    let payload = obj.get("payload")?.as_object()?;
    let role = payload.get("role")?.as_str()?;
    
    if role != "user" && role != "assistant" {
        return None;
    }
    
    let timestamp = obj.get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    
    let content_value = payload.get("content")?;
    let content = extract_text(content_value);
    
    if content.is_empty() {
        return None;
    }
    
    Some((role.to_string(), content, timestamp))
}

/// 解析 Gemini 消息
fn parse_gemini_message(msg: &serde_json::Value) -> Option<(String, String, String)> {
    let obj = msg.as_object()?;
    
    let msg_type = obj.get("type")?.as_str()?;
    let content = obj.get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let timestamp = obj.get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    
    if content.is_empty() {
        return None;
    }
    
    match msg_type {
        "user" => Some(("user".to_string(), content, timestamp)),
        "gemini" => Some(("assistant".to_string(), content, timestamp)),
        _ => None,
    }
}

// ==================== 对话构建 ====================

/// 从解析的消息列表构建对话
fn build_conversation(
    session_id: &str,
    project_name: &str,
    source: &str,
    messages: Vec<(String, String, String)>,
) -> Option<Conversation> {
    if messages.is_empty() {
        return None;
    }
    
    let mut conv = Conversation::new(session_id, project_name, source);
    
    for (role, content, timestamp) in messages {
        conv.add_message(&role, &content, &timestamp);
    }
    
    if !conv.messages.is_empty() {
        conv.generate_title();
        Some(conv)
    } else {
        None
    }
}

// ==================== 数据加载器 ====================

/// 加载 Claude 数据
fn load_claude(config: &SourceConfig) -> (HashMap<String, Vec<Conversation>>, Vec<SkippedFile>) {
    let mut projects: HashMap<String, Vec<Conversation>> = HashMap::new();
    let mut skipped: Vec<SkippedFile> = Vec::new();
    
    let base_dir = PathBuf::from(&config.base_dir);
    
    // 加载 projects 目录
    if let Some(ref projects_subdir) = config.projects_subdir {
        let projects_dir = base_dir.join(projects_subdir);
        if projects_dir.exists() {
            // 收集所有需要处理的文件
            let files_to_process: Vec<_> = WalkDir::new(&projects_dir)
                .min_depth(2)
                .max_depth(2)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().map_or(false, |ext| ext == "jsonl")
                        && !e.file_name().to_string_lossy().starts_with("agent-")
                        && e.metadata().map_or(false, |m| m.len() > 0)
                })
                .collect();
            
            // 并行处理文件
            let results: Vec<_> = files_to_process
                .par_iter()
                .filter_map(|entry| {
                    let path = entry.path();
                    let project_dir = path.parent()?;
                    let project_name = decode_project_name(&project_dir.file_name()?.to_string_lossy());
                    
                    let raw_data = load_jsonl(path);
                    if raw_data.is_empty() {
                        return Some(Err(SkippedFile {
                            file: path.to_string_lossy().to_string(),
                            project: project_name,
                            reason: "empty or invalid".to_string(),
                        }));
                    }
                    
                    let messages: Vec<_> = raw_data
                        .iter()
                        .filter_map(parse_claude_entry)
                        .collect();
                    
                    let session_id = path.file_stem()?.to_string_lossy().to_string();
                    
                    match build_conversation(&session_id, &project_name, "claude", messages) {
                        Some(conv) => Some(Ok((project_name, conv))),
                        None => Some(Err(SkippedFile {
                            file: path.to_string_lossy().to_string(),
                            project: project_name,
                            reason: "no valid messages".to_string(),
                        })),
                    }
                })
                .collect();
            
            // 合并结果
            for result in results {
                match result {
                    Ok((project_name, conv)) => {
                        projects.entry(project_name).or_default().push(conv);
                    }
                    Err(skip) => {
                        skipped.push(skip);
                    }
                }
            }
        }
    }
    
    // 加载 transcripts 目录
    if let Some(ref transcripts_subdir) = config.transcripts_subdir {
        let transcripts_dir = base_dir.join(transcripts_subdir);
        if transcripts_dir.exists() {
            let files: Vec<_> = WalkDir::new(&transcripts_dir)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name().to_string_lossy();
                    name.starts_with("ses_") && name.ends_with(".jsonl")
                })
                .collect();
            
            let results: Vec<_> = files
                .par_iter()
                .filter_map(|entry| {
                    let path = entry.path();
                    let session_id = path.file_stem()?
                        .to_string_lossy()
                        .strip_prefix("ses_")?
                        .to_string();
                    
                    let raw_data = load_jsonl(path);
                    let messages: Vec<_> = raw_data
                        .iter()
                        .filter_map(parse_claude_entry)
                        .collect();
                    
                    build_conversation(&session_id, "Transcripts", "claude", messages)
                        .map(|conv| ("Transcripts".to_string(), conv))
                })
                .collect();
            
            for (project_name, conv) in results {
                projects.entry(project_name).or_default().push(conv);
            }
        }
    }
    
    (projects, skipped)
}

/// 加载 Codex 数据
fn load_codex(config: &SourceConfig) -> (HashMap<String, Vec<Conversation>>, Vec<SkippedFile>) {
    let mut projects: HashMap<String, Vec<Conversation>> = HashMap::new();
    let skipped: Vec<SkippedFile> = Vec::new();
    
    let base_dir = PathBuf::from(&config.base_dir);
    
    if let Some(ref sessions_subdir) = config.sessions_subdir {
        let sessions_dir = base_dir.join(sessions_subdir);
        if sessions_dir.exists() {
            let files: Vec<_> = WalkDir::new(&sessions_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name().to_string_lossy();
                    name.starts_with("rollout-") && name.ends_with(".jsonl")
                        && e.metadata().map_or(false, |m| m.len() > 0)
                })
                .collect();
            
            let results: Vec<_> = files
                .par_iter()
                .filter_map(|entry| {
                    let path = entry.path();
                    let raw_data = load_jsonl(path);
                    if raw_data.is_empty() {
                        return None;
                    }
                    
                    // 从 session_meta 获取 cwd 作为项目名
                    let project_name = raw_data
                        .iter()
                        .find(|d| d.get("type").and_then(|v| v.as_str()) == Some("session_meta"))
                        .and_then(|d| d.get("payload"))
                        .and_then(|p| p.get("cwd"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Codex Sessions")
                        .to_string();
                    
                    let messages: Vec<_> = raw_data
                        .iter()
                        .filter_map(parse_codex_entry)
                        .collect();
                    
                    let session_id = path.file_stem()?.to_string_lossy().to_string();
                    
                    build_conversation(&session_id, &project_name, "codex", messages)
                        .map(|conv| (project_name, conv))
                })
                .collect();
            
            for (project_name, conv) in results {
                projects.entry(project_name).or_default().push(conv);
            }
        }
    }
    
    (projects, skipped)
}

/// 加载 Gemini 数据
fn load_gemini(config: &SourceConfig) -> (HashMap<String, Vec<Conversation>>, Vec<SkippedFile>) {
    let mut projects: HashMap<String, Vec<Conversation>> = HashMap::new();
    let skipped: Vec<SkippedFile> = Vec::new();
    
    let base_dir = PathBuf::from(&config.base_dir);
    
    if let Some(ref tmp_subdir) = config.tmp_subdir {
        let tmp_dir = base_dir.join(tmp_subdir);
        if tmp_dir.exists() {
            // 遍历 hash 目录
            let hash_dirs: Vec<_> = fs::read_dir(&tmp_dir)
                .ok()
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path().is_dir()
                                && e.file_name().to_string_lossy().len() == 64
                        })
                        .collect()
                })
                .unwrap_or_default();
            
            let results: Vec<_> = hash_dirs
                .par_iter()
                .flat_map(|hash_dir| {
                    let chats_dir = hash_dir.path().join("chats");
                    if !chats_dir.exists() {
                        return Vec::new();
                    }
                    
                    fs::read_dir(&chats_dir)
                        .ok()
                        .map(|entries| {
                            entries
                                .filter_map(|e| e.ok())
                                .filter(|e| {
                                    let name = e.file_name().to_string_lossy().to_string();
                                    name.starts_with("session-") && name.ends_with(".json")
                                })
                                .filter_map(|entry| {
                                    let path = entry.path();
                                    let data = load_json(&path)?;
                                    let obj = data.as_object()?;
                                    
                                    let session_id = obj.get("sessionId")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| {
                                            path.file_stem()
                                                .map(|s| s.to_string_lossy().to_string())
                                                .unwrap_or_else(|| "unknown".to_string())
                                        });
                                    
                                    let messages_arr = obj.get("messages")?.as_array()?;
                                    let messages: Vec<_> = messages_arr
                                        .iter()
                                        .filter_map(parse_gemini_message)
                                        .collect();
                                    
                                    let mut conv = build_conversation(
                                        &session_id,
                                        "Gemini Chats",
                                        "gemini",
                                        messages,
                                    )?;
                                    
                                    // 使用 startTime 作为时间戳
                                    if let Some(start_time) = obj.get("startTime").and_then(|v| v.as_str()) {
                                        conv.timestamp = start_time.to_string();
                                    }
                                    
                                    Some(("Gemini Chats".to_string(), conv))
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .collect();
            
            for (project_name, conv) in results {
                projects.entry(project_name).or_default().push(conv);
            }
        }
    }
    
    (projects, skipped)
}

// ==================== 公共接口 ====================

/// 加载所有数据（主入口）
pub fn load_all_data(source: &str) -> LoadedData {
    // 1. 尝试读取缓存
    {
        let cache = DATA_CACHE.read().map_err(|e| {
            log::error!("RwLock poisoned: {}", e);
            e
        }).ok();
        
        if let Some(c) = cache {
            if let Some(data) = c.get(source) {
                return data.clone();
            }
        }
    }
    
    // 2. 缓存未中，执行预检
    let config = match get_source_config(source) {
        Some(c) => c,
        None => {
            return LoadedData {
                source: source.to_string(),
                error: Some(format!("Unknown source: {}", source)),
                ..Default::default()
            };
        }
    };
    
    let base_path = PathBuf::from(&config.base_dir);
    if !base_path.exists() {
        return LoadedData {
            source: source.to_string(),
            error: Some(format!("Directory not found: {}", config.base_dir)),
            ..Default::default()
        };
    }
    
    // 3. 执行加载逻辑
    log::info!("Loading data from source: {}", source);
    let (mut projects, skipped) = match source {
        "claude" => load_claude(&config),
        "codex" => load_codex(&config),
        "gemini" => load_gemini(&config),
        _ => (HashMap::new(), Vec::new()),
    };
    
    // 排序每个项目的对话（按时间倒序）
    for convs in projects.values_mut() {
        convs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    }
    
    let data = LoadedData {
        projects,
        skipped_files: skipped,
        source: source.to_string(),
        error: None,
    };
    
    // 4. 更新缓存
    if let Ok(mut cache) = DATA_CACHE.write() {
        cache.insert(source.to_string(), data.clone());
    } else {
        log::error!("Failed to acquire write lock for DATA_CACHE");
    }
    
    data
}

/// 清除缓存
pub fn clear_cache() {
    if let Ok(mut cache) = DATA_CACHE.write() {
        cache.clear();
    } else {
        log::error!("Failed to clear cache: RwLock poisoned");
    }
}

/// 内部辅助函数：以只读方式访问数据并执行操作，避免全量克隆
fn with_loaded_data<F, R>(source: &str, f: F) -> R 
where 
    F: FnOnce(&LoadedData) -> R,
    R: Default 
{
    // 先检查缓存
    {
        if let Ok(cache) = DATA_CACHE.read() {
            if let Some(data) = cache.get(source) {
                return f(data);
            }
        }
    }

    // 缓存未命中，加载数据
    let data = load_all_data(source);
    f(&data)
}

/// 获取统计信息
pub fn get_stats(source: &str) -> Stats {
    let start = Instant::now();
    with_loaded_data(source, |data| {
        let load_time = start.elapsed().as_secs_f64();
        let projects_count = data.projects.len();
        let conversations_count: usize = data.projects.values().map(|v| v.len()).sum();
        let messages_count: usize = data.projects
            .values()
            .flat_map(|v| v.iter())
            .map(|c| c.messages.len())
            .sum();
        
        Stats {
            source: source.to_string(),
            projects_count,
            conversations_count,
            messages_count,
            conversations_loaded: conversations_count,
            skipped_count: data.skipped_files.len(),
            load_time,
            error: data.error.clone(),
        }
    })
}

/// 获取项目列表
pub fn get_projects_list(source: &str) -> Vec<ProjectInfo> {
    with_loaded_data(source, |data| {
        if let Some(ref err) = data.error {
            return vec![ProjectInfo {
                name: format!("Error: {}", err),
                conversation_count: 0,
                latest_date: "N/A".to_string(),
            }];
        }
        
        let mut projects: Vec<_> = data.projects
            .iter()
            .map(|(name, convs)| {
                let latest_date = convs
                    .first()
                    .map(|c| c.date())
                    .unwrap_or_else(|| "N/A".to_string());
                
                ProjectInfo {
                    name: name.clone(),
                    conversation_count: convs.len(),
                    latest_date,
                }
            })
            .collect();
        
        projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        projects
    })
}

/// 获取项目的对话列表
pub fn get_project_conversations(source: &str, project: &str) -> Vec<ConversationSummary> {
    with_loaded_data(source, |data| {
        data.projects
            .get(project)
            .map(|convs| {
                convs.iter().map(|c| ConversationSummary {
                    session_id: c.session_id.clone(),
                    project_path: c.project_path.clone(),
                    source_type: c.source_type.clone(),
                    title: c.title.clone(),
                    timestamp: c.timestamp.clone(),
                    message_count: c.messages.len(),
                    date: c.date(),
                }).collect()
            })
            .unwrap_or_default()
    })
}

/// 获取对话详情
pub fn get_conversation(source: &str, project: &str, session_id: &str) -> Option<Conversation> {
    with_loaded_data(source, |data| {
        data.projects
            .get(project)?
            .iter()
            .find(|c| c.session_id == session_id)
            .cloned()
    })
}

/// 搜索对话
pub fn search_conversations(source: &str, query: &str) -> Vec<SearchResult> {
    if query.is_empty() {
        return Vec::new();
    }
    
    // 防止 ReDoS：转义用户输入的正则表达式特殊字符
    let escaped_query = regex::escape(query);
    let pattern = match regex::RegexBuilder::new(&escaped_query)
        .case_insensitive(true)
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            log::error!("Invalid regex pattern generated: {}", e);
            return Vec::new();
        }
    };
    
    with_loaded_data(source, |data| {
        data.projects
            .iter()
            .flat_map(|(project_name, convs)| {
                convs.iter().filter_map(|conv| {
                    if pattern.is_match(&conv.title) {
                        Some(SearchResult {
                            project: project_name.clone(),
                            session_id: conv.session_id.clone(),
                            title: conv.title.clone(),
                            date: conv.date(),
                        })
                    } else {
                        None
                    }
                })
            })
            .collect()
    })
}
