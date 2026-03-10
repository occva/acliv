// src/lib/api.ts
import { invoke } from '@tauri-apps/api/core';

// ==================== 类型定义 ====================

/** 会话元信息（来自 Rust SessionMeta） */
export interface SessionMeta {
    providerId: string;       // 'claude' | 'codex' | 'gemini' | 'openclaw' | 'opencode'
    sessionId: string;
    title?: string;           // 提取自第一条用户消息
    summary?: string;         // 截断后的摘要
    projectDir?: string | null; // Gemini 为 null（目录哈希不可逆）
    createdAt?: number;       // 毫秒时间戳
    lastActiveAt?: number;    // 毫秒时间戳
    sourcePath?: string;      // 源文件路径，用于加载消息详情
    resumeCommand?: string;   // 恢复会话的 CLI 命令
}

/** 会话消息 */
export interface SessionMessage {
    role: string;    // 'user' | 'assistant' | 'tool' | 'system'
    content: string;
    ts?: number;     // 毫秒时间戳（可选）
}

export interface DeleteSessionOptions {
    providerId: string;
    sessionId: string;
    sourcePath: string;
}

// ==================== API 函数 ====================

/** 扫描并获取所有会话列表（已按 lastActiveAt 降序排列） */
export async function listSessions(): Promise<SessionMeta[]> {
    return invoke('list_sessions');
}

/** 获取指定会话的消息列表 */
export async function getSessionMessages(
    providerId: string,
    sourcePath: string,
): Promise<SessionMessage[]> {
    return invoke('get_session_messages', { providerId, sourcePath });
}

/** 删除指定会话及 provider 关联资源 */
export async function deleteSession(
    options: DeleteSessionOptions,
): Promise<boolean> {
    const { providerId, sessionId, sourcePath } = options;
    return invoke('delete_session', { providerId, sessionId, sourcePath });
}

/**
 * 在终端执行恢复命令（仅 Windows）
 * 非 Windows 会抛出错误，前端应降级为复制到剪贴板
 */
export async function launchTerminal(
    command: string,
    cwd?: string | null,
): Promise<boolean> {
    return invoke('launch_session_terminal', { command, cwd });
}
