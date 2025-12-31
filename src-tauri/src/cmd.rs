// ===============================
// Tauri Commands - API 接口
// ===============================

use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::loader;
use crate::models::*;

/// 重新加载数据的响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadResponse {
    pub success: bool,
    pub source: String,
    pub load_time: f64,
    pub projects_count: usize,
    pub conversations_count: usize,
    pub messages_count: usize,
    pub skipped_count: usize,
}

// ==================== 辅助函数 ====================

fn validate_string(val: &str, field_name: &str, max_len: usize) -> Result<(), String> {
    let trimmed = val.trim();
    if trimmed.is_empty() {
        return Err(format!("{} cannot be empty", field_name));
    }
    if trimmed.len() > max_len {
        return Err(format!("{} is too long (max {} characters)", field_name, max_len));
    }
    Ok(())
}

// ==================== Tauri Commands ====================

/// 获取统计信息
#[tauri::command]
pub fn get_stats(source: Option<String>) -> Result<Stats, String> {
    let source = source.as_deref().unwrap_or("claude");
    Ok(loader::get_stats(source))
}

/// 获取项目列表
#[tauri::command]
pub fn get_projects(source: Option<String>) -> Result<Vec<ProjectInfo>, String> {
    let source = source.as_deref().unwrap_or("claude");
    Ok(loader::get_projects_list(source))
}

/// 获取项目的对话列表
#[tauri::command]
pub fn get_conversations(source: Option<String>, project: String) -> Result<Vec<ConversationSummary>, String> {
    validate_string(&project, "Project name", 255)?;
    let source = source.as_deref().unwrap_or("claude");
    Ok(loader::get_project_conversations(source, &project))
}

/// 获取对话详情
#[tauri::command]
pub fn get_conversation_detail(
    source: Option<String>,
    project: String,
    session_id: String,
) -> Result<Option<Conversation>, String> {
    validate_string(&project, "Project name", 255)?;
    validate_string(&session_id, "Session ID", 128)?;
    
    let source = source.as_deref().unwrap_or("claude");
    Ok(loader::get_conversation(source, &project, &session_id))
}

/// 搜索对话
#[tauri::command]
pub fn search(source: Option<String>, query: String) -> Result<Vec<SearchResult>, String> {
    // 允许空查询返回空结果，不报错
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    if query.len() > 100 {
        return Err("Search query too long".to_string());
    }
    
    let source = source.as_deref().unwrap_or("claude");
    Ok(loader::search_conversations(source, &query))
}

/// 重新加载数据
#[tauri::command]
pub fn reload_data(source: Option<String>) -> Result<ReloadResponse, String> {
    let source = source.as_deref().unwrap_or("claude");
    
    // 清除缓存
    loader::clear_cache();
    
    // 重新加载
    let start = Instant::now();
    let data = loader::load_all_data(source);
    let load_time = start.elapsed().as_secs_f64();
    
    let projects_count = data.projects.len();
    let conversations_count: usize = data.projects.values().map(|v| v.len()).sum();
    let messages_count: usize = data.projects
        .values()
        .flat_map(|v| v.iter())
        .map(|c| c.messages.len())
        .sum();
    
    Ok(ReloadResponse {
        success: data.error.is_none(),
        source: source.to_string(),
        load_time,
        projects_count,
        conversations_count,
        messages_count,
        skipped_count: data.skipped_files.len(),
    })
}

/// 列出所有数据源
#[tauri::command]
pub fn list_sources() -> Result<Vec<&'static str>, String> {
    Ok(loader::list_sources())
}
