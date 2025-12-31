// ===============================
// Data Models - 数据结构定义
// ===============================

use serde::{Deserialize, Serialize};


/// 消息模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub timestamp: String,
}

impl Message {}

/// 对话模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub session_id: String,
    pub project_path: String,
    pub source_type: String,
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub timestamp: String,
}

impl Conversation {
    pub fn new(session_id: impl Into<String>, project_path: impl Into<String>, source_type: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            project_path: project_path.into(),
            source_type: source_type.into(),
            messages: Vec::new(),
            title: String::new(),
            timestamp: String::new(),
        }
    }

    pub fn add_message(&mut self, role: impl Into<String>, content: impl Into<String>, timestamp: impl Into<String>) {
        let ts: String = timestamp.into();
        // 如果对话没有时间戳，使用第一条消息的时间戳
        if self.timestamp.is_empty() && !ts.is_empty() {
            self.timestamp = ts.clone();
        }
        self.messages.push(Message {
            role: role.into(),
            content: content.into(),
            timestamp: ts,
        });
    }

    pub fn generate_title(&mut self) {
        for msg in &self.messages {
            if msg.role == "user" && !msg.content.is_empty() {
                let content: String = msg.content.split_whitespace().collect::<Vec<_>>().join(" ");
                // 安全截取 UTF-8 字符串（按字符数而非字节数）
                let title = if content.chars().count() > 80 {
                    let truncated: String = content.chars().take(77).collect();
                    format!("{}...", truncated)
                } else {
                    content
                };
                self.title = title;
                return;
            }
        }
        // 安全截取 session_id
        let id_len = self.session_id.chars().count().min(8);
        let short_id: String = self.session_id.chars().take(id_len).collect();
        self.title = format!("Session {}", short_id);
    }



    pub fn date(&self) -> String {
        if self.timestamp.is_empty() {
            "N/A".to_string()
        } else {
            // 安全截取前 10 个字符
            self.timestamp.chars().take(10).collect()
        }
    }
}

/// 对话摘要（用于列表显示）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationSummary {
    pub session_id: String,
    pub project_path: String,
    pub source_type: String,
    pub title: String,
    pub timestamp: String,
    pub message_count: usize,
    pub date: String,
}

impl From<Conversation> for ConversationSummary {
    fn from(conv: Conversation) -> Self {
        let message_count = conv.messages.len();
        let date = if conv.timestamp.is_empty() {
            "N/A".to_string()
        } else {
            // 安全截取前 10 个字符
            conv.timestamp.chars().take(10).collect()
        };
        Self {
            session_id: conv.session_id,
            project_path: conv.project_path,
            source_type: conv.source_type,
            title: conv.title,
            timestamp: conv.timestamp,
            message_count,
            date,
        }
    }
}

/// 项目信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectInfo {
    pub name: String,
    pub conversation_count: usize,
    pub latest_date: String,
}

/// 统计信息
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    pub source: String,
    pub projects_count: usize,
    pub conversations_count: usize,
    pub messages_count: usize,
    pub conversations_loaded: usize,
    pub skipped_count: usize,
    pub load_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}



/// 数据源配置
#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub base_dir: String,
    pub projects_subdir: Option<String>,
    pub transcripts_subdir: Option<String>,
    pub sessions_subdir: Option<String>,
    pub tmp_subdir: Option<String>,
}

/// 搜索结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchResult {
    pub project: String,
    pub session_id: String,
    pub title: String,
    pub date: String,
}
