import { invoke } from '@tauri-apps/api/core';

export const WEB_TOKEN_STORAGE_KEY = 'aichv_token';

// ==================== 类型定义 ====================

/** 会话元信息（来自 Rust SessionMeta） */
export interface SessionMeta {
    providerId: string;       // 'claude' | 'codex' | 'gemini' | 'openclaw' | 'opencode'
    sessionId: string;
    title?: string;           // 提取自第一条用户消息
    summary?: string;         // 截断后的摘要
    projectDir?: string | null; // Gemini 为 null（目录哈希不可逆）
    cwd?: string | null;
    model?: string | null;
    createdAt?: number;       // 毫秒时间戳
    lastActiveAt?: number;    // 毫秒时间戳
    sourcePath?: string;      // 源文件路径，用于加载消息详情
    resumeCommand?: string;   // 恢复会话的 CLI 命令
}

/** 会话消息 */
export interface SessionMessage {
    role: string;    // 'user' | 'assistant' | 'tool' | 'system'
    kind?: string;
    name?: string;
    callId?: string;
    content: string;
    ts?: number;     // 毫秒时间戳（可选）
}

export interface DeleteSessionOptions {
    providerId: string;
    sessionId: string;
    sourcePath: string;
}

export interface SearchIndexStatus {
    dbPath: string;
    ready: boolean;
    sourcesCount: number;
    projectsCount: number;
    sessionsCount: number;
    messagesCount: number;
    lastIndexedAt?: string;
    lastSuccessfulSyncAt?: string;
    lastErrorAt?: string;
    dbSizeBytes: number;
    errorCount: number;
    sources: Array<{
        providerId: string;
        projectsCount: number;
        sessionsCount: number;
        messagesCount: number;
    }>;
}

export interface RebuildSearchIndexResult {
    dbPath: string;
    sourcesCount: number;
    projectsCount: number;
    indexedSessions: number;
    indexedMessages: number;
    errorSessions: number;
}

export interface RefreshSearchIndexResult {
    dbPath: string;
    scannedSessions: number;
    addedSessions: number;
    updatedSessions: number;
    removedSessions: number;
    skippedSessions: number;
    errorSessions: number;
    indexedSessions: number;
    indexedMessages: number;
}

export interface IndexedSourceRef {
    providerId: string;
    sourcePath: string;
}

export interface SearchFragmentHit {
    rank: number;
    providerId: string;
    sessionId: string;
    sourcePath: string;
    sessionTitle: string;
    project: string;
    lastActiveAt?: number;
    snippet: string;
    messageRole: string;
    messageTimestamp?: number;
    seq: number;
}

export interface SearchContentResult {
    totalCount: number;
    hits: SearchFragmentHit[];
}

export interface IndexedMessage {
    msgUuid?: string;
    parentUuid?: string;
    role: string;
    kind: string;
    name?: string;
    callId?: string;
    content: string;
    ts?: number;
    isSidechain: boolean;
    toolNames: string[];
    seq: number;
}

export interface IndexedSession {
    providerId: string;
    sessionId: string;
    sourcePath: string;
    title?: string;
    summary?: string;
    resumeCommand?: string;
    cwd?: string;
    model?: string;
    project: string;
    projectName: string;
    createdAt?: number;
    lastActiveAt?: number;
    messageCount: number;
    hasToolUse: boolean;
}

export interface IndexedProjectOption {
    project: string;
    projectName: string;
    sessionsCount: number;
}

export interface PagedIndexedSessionsResult {
    totalCount: number;
    items: IndexedSession[];
}

interface ApiResponse<T> {
    ok: boolean;
    data: T;
    error?: string;
}

interface BackendAdapter {
    listSessions: () => Promise<SessionMeta[]>;
    listIndexedSessions: (
        providerId?: string | null,
        limit?: number | null,
    ) => Promise<IndexedSession[]>;
    listIndexedSessionsPage: (
        providerId?: string | null,
        projectPath?: string | null,
        limit?: number | null,
        offset?: number | null,
    ) => Promise<PagedIndexedSessionsResult>;
    listIndexedProjects: (
        providerId?: string | null,
    ) => Promise<IndexedProjectOption[]>;
    listIndexedSessionsBySourcePaths: (
        providerId: string,
        sourcePaths: string[],
    ) => Promise<IndexedSession[]>;
    getSessionMessages: (providerId: string, sourcePath: string) => Promise<SessionMessage[]>;
    getIndexedSessionMessages: (providerId: string, sourcePath: string) => Promise<IndexedMessage[]>;
    searchContent: (
        query: string,
        providerId?: string | null,
        sinceTs?: number | null,
        projectPath?: string | null,
        limit?: number | null,
        sortBy?: 'relevance' | 'recent' | null,
    ) => Promise<SearchContentResult>;
    rebuildSearchIndex: () => Promise<RebuildSearchIndexResult>;
    refreshSearchIndex: () => Promise<RefreshSearchIndexResult>;
    getSearchIndexStatus: () => Promise<SearchIndexStatus>;
    deleteSession: (options: DeleteSessionOptions) => Promise<boolean>;
    launchTerminal: (
        command: string,
        cwd?: string | null,
        terminalKind?: 'cmd' | 'powershell' | null,
    ) => Promise<boolean>;
    openInFileExplorer: (path: string) => Promise<boolean>;
}

function isTauriRuntime(): boolean {
    if (typeof window === 'undefined') return false;
    const win = window as unknown as Record<string, unknown>;
    return Boolean(win.__TAURI_INTERNALS__ || win.__TAURI__ || win.__TAURI_IPC__);
}

export function isWebMode(): boolean {
    return !isTauriRuntime();
}

function getWebToken(): string {
    if (typeof window === 'undefined') return '';
    return localStorage.getItem(WEB_TOKEN_STORAGE_KEY)?.trim() ?? '';
}

function assertWebToken(): string {
    const token = getWebToken();
    if (!token) {
        throw new Error('Missing web token. Set ?token=... once, then refresh.');
    }
    return token;
}

async function fetchApi<T>(path: string, init: RequestInit): Promise<T> {
    const token = assertWebToken();
    const headers = new Headers(init.headers);
    headers.set('Authorization', `Bearer ${token}`);
    if (init.body && !headers.has('Content-Type')) {
        headers.set('Content-Type', 'application/json');
    }

    const response = await fetch(path, {
        ...init,
        headers,
    });

    let payload: ApiResponse<T> | null = null;
    try {
        payload = await response.json() as ApiResponse<T>;
    } catch {
        // keep payload as null and throw fallback message below
    }

    if (!response.ok || !payload?.ok) {
        const errorMessage = payload?.error ?? `Request failed: ${response.status}`;
        throw new Error(errorMessage);
    }

    return payload.data;
}

const tauriAdapter: BackendAdapter = {
    listSessions: () => invoke('list_sessions'),
    listIndexedSessions: (providerId, limit) =>
        invoke('list_indexed_sessions', { providerId, limit }),
    listIndexedSessionsPage: (providerId, projectPath, limit, offset) =>
        invoke('list_indexed_sessions_page', { providerId, projectPath, limit, offset }),
    listIndexedProjects: (providerId) =>
        invoke('list_indexed_projects', { providerId }),
    listIndexedSessionsBySourcePaths: (providerId, sourcePaths) =>
        invoke('list_indexed_sessions_by_source_paths', { providerId, sourcePaths }),
    getSessionMessages: (providerId, sourcePath) =>
        invoke('get_session_messages', { providerId, sourcePath }),
    getIndexedSessionMessages: (providerId, sourcePath) =>
        invoke('get_indexed_session_messages', { providerId, sourcePath }),
    searchContent: (query, providerId, sinceTs, projectPath, limit, sortBy) =>
        invoke('search_content', { query, providerId, sinceTs, projectPath, limit, sortBy }),
    rebuildSearchIndex: () => invoke('rebuild_search_index'),
    refreshSearchIndex: () => invoke('refresh_search_index'),
    getSearchIndexStatus: () => invoke('get_search_index_status'),
    deleteSession: ({ providerId, sessionId, sourcePath }) =>
        invoke('delete_session', { providerId, sessionId, sourcePath }),
    launchTerminal: (command, cwd, terminalKind) =>
        invoke('launch_session_terminal', { command, cwd, terminalKind }),
    openInFileExplorer: (path) =>
        invoke('open_in_file_explorer', { path }),
};

const webAdapter: BackendAdapter = {
    listSessions: () => fetchApi('/api/sessions', { method: 'GET' }),
    listIndexedSessions: (providerId, limit) => {
        const params = new URLSearchParams();
        if (providerId?.trim()) params.set('providerId', providerId.trim());
        if (limit != null) params.set('limit', String(limit));
        const query = params.toString();
        const path = query ? `/api/search/index/sessions?${query}` : '/api/search/index/sessions';
        return fetchApi(path, { method: 'GET' });
    },
    listIndexedSessionsPage: (providerId, projectPath, limit, offset) => {
        const params = new URLSearchParams();
        if (providerId?.trim()) params.set('providerId', providerId.trim());
        if (projectPath?.trim()) params.set('projectPath', projectPath.trim());
        if (limit != null) params.set('limit', String(limit));
        if (offset != null) params.set('offset', String(offset));
        const query = params.toString();
        const path = query
            ? `/api/search/index/sessions/page?${query}`
            : '/api/search/index/sessions/page';
        return fetchApi(path, { method: 'GET' });
    },
    listIndexedProjects: (providerId) => {
        const params = new URLSearchParams();
        if (providerId?.trim()) params.set('providerId', providerId.trim());
        const query = params.toString();
        const path = query
            ? `/api/search/index/projects?${query}`
            : '/api/search/index/projects';
        return fetchApi(path, { method: 'GET' });
    },
    listIndexedSessionsBySourcePaths: (providerId, sourcePaths) =>
        fetchApi('/api/search/index/sessions/by-paths', {
            method: 'POST',
            body: JSON.stringify({ providerId, sourcePaths }),
        }),
    getSessionMessages: (providerId, sourcePath) =>
        fetchApi('/api/session/messages', {
            method: 'POST',
            body: JSON.stringify({ providerId, sourcePath }),
        }),
    getIndexedSessionMessages: (providerId, sourcePath) =>
        fetchApi('/api/search/index/session/messages', {
            method: 'POST',
            body: JSON.stringify({ providerId, sourcePath }),
        }),
    searchContent: (query, providerId, sinceTs, projectPath, limit, sortBy) =>
        fetchApi('/api/search/content', {
            method: 'POST',
            body: JSON.stringify({ query, providerId, sinceTs, projectPath, limit, sortBy }),
        }),
    rebuildSearchIndex: () =>
        fetchApi('/api/search/index/rebuild', {
            method: 'POST',
        }),
    refreshSearchIndex: () =>
        fetchApi('/api/search/index/refresh', {
            method: 'POST',
        }),
    getSearchIndexStatus: () =>
        fetchApi('/api/search/index/status', { method: 'GET' }),
    deleteSession: ({ providerId, sessionId, sourcePath }) =>
        fetchApi('/api/session/delete', {
            method: 'POST',
            body: JSON.stringify({ providerId, sessionId, sourcePath }),
        }),
    launchTerminal: async () => {
        throw new Error('Not supported in web mode');
    },
    openInFileExplorer: async () => {
        throw new Error('Not supported in web mode');
    },
};

function getAdapter(): BackendAdapter {
    return isTauriRuntime() ? tauriAdapter : webAdapter;
}

// ==================== API 函数 ====================

/** 扫描并获取所有会话列表（已按 lastActiveAt 降序排列） */
export async function listSessions(): Promise<SessionMeta[]> {
    return getAdapter().listSessions();
}

/** 获取指定会话的消息列表 */
export async function getSessionMessages(
    providerId: string,
    sourcePath: string,
): Promise<SessionMessage[]> {
    return getAdapter().getSessionMessages(providerId, sourcePath);
}

export async function listIndexedSessions(
    providerId?: string | null,
    limit?: number | null,
): Promise<IndexedSession[]> {
    return getAdapter().listIndexedSessions(providerId, limit);
}

export async function listIndexedSessionsPage(
    providerId?: string | null,
    projectPath?: string | null,
    limit?: number | null,
    offset?: number | null,
): Promise<PagedIndexedSessionsResult> {
    return getAdapter().listIndexedSessionsPage(providerId, projectPath, limit, offset);
}

export async function listIndexedProjects(
    providerId?: string | null,
): Promise<IndexedProjectOption[]> {
    return getAdapter().listIndexedProjects(providerId);
}

export async function listIndexedSessionsBySourcePaths(
    providerId: string,
    sourcePaths: string[],
): Promise<IndexedSession[]> {
    return getAdapter().listIndexedSessionsBySourcePaths(providerId, sourcePaths);
}

export async function getIndexedSessionMessages(
    providerId: string,
    sourcePath: string,
): Promise<IndexedMessage[]> {
    return getAdapter().getIndexedSessionMessages(providerId, sourcePath);
}

export async function searchContent(
    query: string,
    providerId?: string | null,
    sinceTs?: number | null,
    projectPath?: string | null,
    limit?: number | null,
    sortBy?: 'relevance' | 'recent' | null,
): Promise<SearchContentResult> {
    return getAdapter().searchContent(query, providerId, sinceTs, projectPath, limit, sortBy);
}

export async function rebuildSearchIndex(): Promise<RebuildSearchIndexResult> {
    return getAdapter().rebuildSearchIndex();
}

export async function refreshSearchIndex(): Promise<RefreshSearchIndexResult> {
    return getAdapter().refreshSearchIndex();
}

export async function getSearchIndexStatus(): Promise<SearchIndexStatus> {
    return getAdapter().getSearchIndexStatus();
}

/** 删除指定会话及 provider 关联资源 */
export async function deleteSession(
    options: DeleteSessionOptions,
): Promise<boolean> {
    return getAdapter().deleteSession(options);
}

/**
 * 在终端执行恢复命令（仅 Tauri 支持）
 * Web 模式会返回 not supported
 */
export async function launchTerminal(
    command: string,
    cwd?: string | null,
    terminalKind?: 'cmd' | 'powershell' | null,
): Promise<boolean> {
    return getAdapter().launchTerminal(command, cwd, terminalKind);
}

/** 在文件管理器中打开目录或定位文件（仅 Tauri 支持） */
export async function openInFileExplorer(path: string): Promise<boolean> {
    return getAdapter().openInFileExplorer(path);
}
