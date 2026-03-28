use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexSourceStat {
    pub provider_id: String,
    pub projects_count: i64,
    pub sessions_count: i64,
    pub messages_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchIndexStatus {
    pub db_path: String,
    pub ready: bool,
    pub sources_count: i64,
    pub projects_count: i64,
    pub sessions_count: i64,
    pub messages_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_indexed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_successful_sync_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error_at: Option<String>,
    pub db_size_bytes: i64,
    pub error_count: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SearchIndexSourceStat>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildSearchIndexResult {
    pub db_path: String,
    pub sources_count: i64,
    pub projects_count: i64,
    pub indexed_sessions: i64,
    pub indexed_messages: i64,
    pub error_sessions: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshSearchIndexResult {
    pub db_path: String,
    pub scanned_sessions: i64,
    pub added_sessions: i64,
    pub updated_sessions: i64,
    pub removed_sessions: i64,
    pub skipped_sessions: i64,
    pub error_sessions: i64,
    pub indexed_sessions: i64,
    pub indexed_messages: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedSourceRef {
    pub provider_id: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedProjectOption {
    pub project: String,
    pub project_name: String,
    pub sessions_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedSession {
    pub provider_id: String,
    pub session_id: String,
    pub source_path: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub project: String,
    pub project_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<i64>,
    pub message_count: i64,
    pub has_tool_use: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagedIndexedSessionsResult {
    pub total_count: i64,
    pub items: Vec<IndexedSession>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_uuid: Option<String>,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
    pub is_sidechain: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_names: Vec<String>,
    pub seq: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFragmentHit {
    pub rank: i64,
    pub provider_id: String,
    pub session_id: String,
    pub source_path: String,
    pub session_title: String,
    pub project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<i64>,
    pub snippet: String,
    pub message_role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_timestamp: Option<i64>,
    pub seq: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchContentResult {
    pub total_count: i64,
    pub hits: Vec<SearchFragmentHit>,
}
