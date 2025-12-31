/**
 * Tauri API 封装 - 与 Rust 后端通信
 */
import { invoke } from '@tauri-apps/api/core';

// ==================== 类型定义 ====================

export interface Stats {
    source: string;
    projects_count: number;
    conversations_count: number;
    messages_count: number;
    conversations_loaded: number;
    skipped_count: number;
    load_time: number;
    error?: string;
}

export interface ProjectInfo {
    name: string;
    conversation_count: number;
    latest_date: string;
}

export interface ConversationSummary {
    session_id: string;
    project_path: string;
    source_type: string;
    title: string;
    timestamp: string;
    message_count: number;
    date: string;
}

export interface Message {
    role: string;
    content: string;
    timestamp: string;
}

export interface Conversation {
    session_id: string;
    project_path: string;
    source_type: string;
    messages: Message[];
    title: string;
    timestamp: string;
}

export interface SearchResult {
    project: string;
    session_id: string;
    title: string;
    date: string;
}

export interface ReloadResponse {
    success: boolean;
    source: string;
    load_time: number;
    projects_count: number;
    conversations_count: number;
    messages_count: number;
    skipped_count: number;
}

// ==================== API 函数 ====================

/**
 * 获取统计信息
 */
export async function getStats(source: string = 'claude'): Promise<Stats> {
    return invoke('get_stats', { source });
}

/**
 * 获取项目列表
 */
export async function getProjects(source: string = 'claude'): Promise<ProjectInfo[]> {
    return invoke('get_projects', { source });
}

/**
 * 获取项目的对话列表
 */
export async function getConversations(source: string, project: string): Promise<ConversationSummary[]> {
    return invoke('get_conversations', { source, project });
}

/**
 * 获取对话详情
 */
export async function getConversationDetail(
    source: string,
    project: string,
    sessionId: string
): Promise<Conversation | null> {
    return invoke('get_conversation_detail', { source, project, sessionId });
}

/**
 * 搜索对话
 */
export async function search(source: string, query: string): Promise<SearchResult[]> {
    return invoke('search', { source, query });
}

/**
 * 重新加载数据
 */
export async function reloadData(source: string = 'claude'): Promise<ReloadResponse> {
    return invoke('reload_data', { source });
}

/**
 * 获取所有数据源列表
 */
export async function listSources(): Promise<string[]> {
    return invoke('list_sources');
}
