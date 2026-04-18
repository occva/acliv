<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import * as api from './lib/api';
  import {
    getInitialLocale,
    setLocale as persistLocale,
    translate,
    type Locale,
    type TranslationKey,
    type TranslationParams,
  } from './lib/i18n';

  // --- Icons from Legacy app.js ---
  const ICONS = {
    project: `<path d="M1.75 1A1.75 1.75 0 0 0 0 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0 0 16 13.25v-8.5A1.75 1.75 0 0 0 14.25 3H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 1.22 5.55 1 5 1H1.75Z"/>`,
    conversation: `<path fill-rule="evenodd" d="M1.75 2.5a.75.75 0 0 0 0 1.5h10.5a.75.75 0 0 0 0-1.5H1.75Zm0 5a.75.75 0 0 0 0 1.5h6a.75.75 0 0 0 0-1.5h-6ZM.5 15.5l3-3h10.75a1.75 1.75 0 0 0 1.75-1.75v-9A1.75 1.75 0 0 0 14.25 0H1.75A1.75 1.75 0 0 0 0 1.75v13.75Z"/>`,
    message: `<path fill-rule="evenodd" d="M0 3.75C0 2.784.784 2 1.75 2h12.5c.966 0 1.75.784 1.75 1.75v8.5A1.75 1.75 0 0 1 14.25 14H1.75A1.75 1.75 0 0 1 0 12.25v-8.5Zm1.75-.25a.25.25 0 0 0-.25.25v8.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25v-8.5a.25.25 0 0 0-.25-.25H1.75ZM3.5 6.25a.75.75 0 0 1 .75-.75h7a.75.75 0 0 1 0 1.5h-7a.75.75 0 0 1-.75-.75Zm.75 2.25a.75.75 0 0 0 0 1.5h4a.75.75 0 0 0 0-1.5h-4Z"/>`,
    error: `<path fill-rule="evenodd" d="M8.22 1.754a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368L7.78 1.754ZM10.5 1.5a1.75 1.75 0 0 0-3 0L1.418 12.875A1.75 1.75 0 0 0 2.918 15h10.164a1.75 1.75 0 0 0 1.5-2.125L8.78 1.754ZM9 10.25a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0v2.5Zm-.75 3.25a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z"/>`,
    clock: `<path fill-rule="evenodd" d="M8 1a7 7 0 1 0 0 14A7 7 0 0 0 8 1ZM2.5 8a5.5 5.5 0 1 1 11 0A5.5 5.5 0 0 1 2.5 8Zm6.25-2.75a.75.75 0 0 0-1.5 0V8c0 .2.08.39.22.53l1.75 1.75a.75.75 0 1 0 1.06-1.06L8.75 7.69V5.25Z"/>`,
    copy: `<path fill-rule="evenodd" d="M5 2.75A1.75 1.75 0 0 1 6.75 1h5.5A1.75 1.75 0 0 1 14 2.75v5.5A1.75 1.75 0 0 1 12.25 10h-5.5A1.75 1.75 0 0 1 5 8.25v-5.5Zm1.75-.25a.25.25 0 0 0-.25.25v5.5c0 .138.112.25.25.25h5.5a.25.25 0 0 0 .25-.25v-5.5a.25.25 0 0 0-.25-.25h-5.5ZM2 5.75C2 4.784 2.784 4 3.75 4a.75.75 0 0 1 0 1.5.25.25 0 0 0-.25.25v6.5c0 .138.112.25.25.25h6.5a.25.25 0 0 0 .25-.25.75.75 0 0 1 1.5 0A1.75 1.75 0 0 1 10.25 14h-6.5A1.75 1.75 0 0 1 2 12.25v-6.5Z"/>`,
    terminal: `<path fill-rule="evenodd" d="M1.75 2A1.75 1.75 0 0 0 0 3.75v8.5C0 13.216.784 14 1.75 14h12.5A1.75 1.75 0 0 0 16 12.25v-8.5A1.75 1.75 0 0 0 14.25 2H1.75Zm0 1.5a.25.25 0 0 0-.25.25v8.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25v-8.5a.25.25 0 0 0-.25-.25H1.75Zm2.72 2.22a.75.75 0 0 1 1.06 0l1.75 1.75a.75.75 0 0 1 0 1.06L5.53 10.28a.75.75 0 1 1-1.06-1.06L5.69 8 4.47 6.78a.75.75 0 0 1 0-1.06ZM8 9.25a.75.75 0 0 1 .75-.75h2.75a.75.75 0 0 1 0 1.5H8.75A.75.75 0 0 1 8 9.25Z"/>`,
    folderOutline: `<g transform="scale(0.015625)"><path d="M919.68 949.12H103.68a96 96 0 0 1-96-96V167.04a96 96 0 0 1 96-96H384a95.36 95.36 0 0 1 72.96 33.92l56.32 64a33.28 33.28 0 0 0 24.32 10.88h378.88a96 96 0 0 1 96.64 96v576a96 96 0 0 1-93.44 97.28zM103.68 135.04a32 32 0 0 0-32 32v686.08a32 32 0 0 0 32 32h816a32.64 32.64 0 0 0 32-32v-576a32 32 0 0 0-32-32H540.8a99.2 99.2 0 0 1-74.24-33.28l-56.32-64a33.92 33.92 0 0 0-26.24-12.8z" fill="currentColor"></path><path d="M945.28 374.4H78.08a32 32 0 1 1 0-64h867.2a32 32 0 0 1 0 64z" fill="currentColor"></path></g>`,
    folderOpen: `<path d="M1.75 1A1.75 1.75 0 0 0 0 2.75v8.5C0 12.216.784 13 1.75 13h8.518a1.75 1.75 0 0 0 1.626-1.102l1.757-4.393A1.75 1.75 0 0 0 12.026 5H7.5a.25.25 0 0 1-.2-.1l-.9-1.2A1.75 1.75 0 0 0 5 3H1.75a.25.25 0 0 1-.25-.25v0A.25.25 0 0 1 1.75 2.5H5a.25.25 0 0 1 .2.1l.9 1.2A1.75 1.75 0 0 0 7.5 4h4.526a3.25 3.25 0 0 1 3.019 4.458l-1.757 4.394A3.25 3.25 0 0 1 10.268 15H1.75A1.75 1.75 0 0 1 0 13.25v-10.5A1.75 1.75 0 0 1 1.75 1Z"/>`,
    trash: `<path d="M6.5 1a1 1 0 0 0-.894.553L5.382 2H2.75a.75.75 0 0 0 0 1.5h.45l.632 9.49A1.75 1.75 0 0 0 5.577 14.5h4.846a1.75 1.75 0 0 0 1.745-1.51L12.8 3.5h.45a.75.75 0 0 0 0-1.5h-2.632l-.224-.447A1 1 0 0 0 9.5 1h-3Zm.276 1.5h2.448l.25.5h-2.948l.25-.5Zm-1.44 1.999h5.328l-.617 9.255a.25.25 0 0 1-.249.216H5.202a.25.25 0 0 1-.249-.216L4.336 4.5Zm1.414 1.25a.75.75 0 0 1 .75.75v4a.75.75 0 0 1-1.5 0v-4a.75.75 0 0 1 .75-.75Zm2.5 0a.75.75 0 0 1 .75.75v4a.75.75 0 0 1-1.5 0v-4a.75.75 0 0 1 .75-.75Z"/>`,
    search: `<path d="M11.742 10.344a6.5 6.5 0 1 0-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 0 0 1.415-1.414l-3.85-3.85a1.007 1.007 0 0 0-.115-.1zM12 6.5a5.5 5.5 0 1 1-11 0 5.5 5.5 0 0 1 11 0z"/>`,
    empty_box: `<path d="M1.75 1h12.5c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0 1 14.25 15H1.75A1.75 1.75 0 0 1 0 13.25V2.75C0 1.784.784 1 1.75 1ZM1.5 2.75v10.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25V2.75a.25.25 0 0 0-.25-.25H1.75a.25.25 0 0 0-.25.25ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"/>`,
    database: `<path d="M8 1.25c-3.59 0-5.75 1.16-5.75 2.5v8.5c0 1.34 2.16 2.5 5.75 2.5s5.75-1.16 5.75-2.5v-8.5c0-1.34-2.16-2.5-5.75-2.5Zm0 1.5c2.91 0 4.25.84 4.25 1s-1.34 1-4.25 1-4.25-.84-4.25-1 1.34-1 4.25-1Zm-4.25 3.1c1.11.63 2.74.9 4.25.9s3.14-.27 4.25-.9v1.4c0 .16-1.34 1-4.25 1s-4.25-.84-4.25-1v-1.4Zm0 3.5c1.11.63 2.74.9 4.25.9s3.14-.27 4.25-.9v1.4c0 .16-1.34 1-4.25 1s-4.25-.84-4.25-1v-1.4Zm4.25 3.9c-2.91 0-4.25-.84-4.25-1v-1.4c1.11.63 2.74.9 4.25.9s3.14-.27 4.25-.9v1.4c0 .16-1.34 1-4.25 1Z"/>`,
    refresh: `<path fill-rule="evenodd" d="M8 2.25a5.75 5.75 0 1 0 5.527 7.344.75.75 0 0 1 1.444.402A7.25 7.25 0 1 1 13.11 3.57v-1.32a.75.75 0 0 1 1.5 0v3.5a.75.75 0 0 1-.75.75h-3.5a.75.75 0 0 1 0-1.5h1.717A5.715 5.715 0 0 0 8 2.25Z"/>`,
    back: `<svg viewBox="0 0 1024 1024" width="14" height="14" fill="currentColor"><path d="M604.8 407.68H158.72L375.68 198.4c17.92-17.28 17.92-46.08 0-63.36a48.384 48.384 0 0 0-65.92 0L13.44 421.12c-17.92 17.28-17.92 46.08 0 63.36l296.32 286.08c17.92 17.28 47.36 17.28 65.92 0 17.92-17.28 17.92-46.08 0-63.36L158.72 497.92h446.08c179.84 0 325.76 140.8 325.76 314.88v44.8c0 24.96 21.12 44.8 46.72 44.8 25.6 0 46.72-20.48 46.72-44.8v-44.8c0-224-187.52-405.12-419.2-405.12z"></path></svg>`,
    dropdown_arrow: `<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M12.78 6.22a.75.75 0 0 1 0 1.06l-4.25 4.25a.75.75 0 0 1-1.06 0L3.22 7.28a.75.75 0 0 1 1.06-1.06L8 9.94l3.72-3.72a.75.75 0 0 1 1.06 0Z"></path></svg>`
  };

  function getIcon(name: keyof typeof ICONS, size = 14) {
    return `<svg width="${size}" height="${size}" viewBox="0 0 16 16" fill="currentColor">${ICONS[name]}</svg>`;
  }

  // ---- 本地类型（与旧 api.ts 同构，模板字段名不变）----
  interface SessionMeta {
    providerId: string; sessionId: string; title?: string; summary?: string;
    projectDir?: string | null; cwd?: string | null; model?: string | null;
    createdAt?: number; lastActiveAt?: number; sourcePath?: string; resumeCommand?: string;
  }
  interface ProjectInfo { name: string; conversation_count: number; latest_date: string; }
  interface ConvSummary {
    session_id: string; project_path: string; source_type: string;
    title: string; timestamp: string; message_count: number; date: string;
  }
  interface Stats { projects_count: number; conversations_count: number; messages_count: number; }
  interface SearchResultLocal {
    project: string;
    source_type: string;
    session_id: string;
    title: string;
    date: string;
    snippet?: string;
    match_role?: string;
    match_seq?: number;
    search_query?: string;
    sort_ts?: number;
  }
  interface ConversationLike {
      session_id: string;
      project_path: string;
      source_type: string;
      title: string;
      timestamp: string;
      messages: ConversationMessageView[];
  }
  interface ConversationMessage {
      role: string;
      kind?: string;
      name?: string;
      callId?: string;
      content: string;
      ts?: number;
      seq: number;
  }
  interface ConversationMessageView {
      role: string;
      kind?: string;
      name?: string;
      callId?: string;
      content: string;
      timestamp: string;
      seq: number;
  }
  interface MessageBlock {
      role: string;
      kind: string;
      name?: string;
      callId?: string;
      content: string;
      timestamp?: string;
      seqStart: number;
      seqEnd: number;
      blocks?: MessageBlock[];
      runCount?: number;
  }
  type UiMessage =
      | { kind: 'key'; key: TranslationKey; params?: TranslationParams }
      | { kind: 'text'; text: string };
  interface SearchIndexSyncEvent {
      phase: string;
      message?: string;
      count?: number;
      total?: number;
      indexedSessions?: number;
      indexedMessages?: number;
      addedSessions?: number;
      updatedSessions?: number;
      removedSessions?: number;
      skippedSessions?: number;
      errorSessions?: number;
      changedSources?: api.IndexedSourceRef[];
      status?: api.SearchIndexStatus;
  }
  type SearchTimeRange = 'all' | '7d' | '30d' | '90d';
  type SearchSort = 'relevance' | 'recent';
  type IndexModalTab = 'overview' | 'sessions';
  type IndexLibraryItem = api.IndexedSession;
  type RouteMode = 'push' | 'replace' | 'none';
  type MarkdownComponentType = typeof import('./lib/components/Markdown.svelte').default;
  interface SelectConversationOptions {
      routeMode?: RouteMode;
      restoreScroll?: boolean;
      scrollToBottom?: boolean;
  }
  interface DetailScrollState {
      top: number;
      atBottom: boolean;
  }
  interface ConversationProgressAnchor {
      key: string;
      label: string;
      preview: string;
      top: number;
  }
  const INDEX_LIBRARY_PAGE_SIZE = 50;
  const AUTO_SYNC_INTERVAL_WEB_MS = 120000;
  const AUTO_SYNC_INTERVAL_DESKTOP_MS = 300000;
  const DETAIL_SCROLL_STORAGE_PREFIX = 'acliv:detail-scroll:';
  const DETAIL_SCROLL_BOTTOM_THRESHOLD = 40;
  const DETAIL_PROGRESS_ANCHOR_OFFSET = 10;
  const DETAIL_PROGRESS_ANCHOR_SPACING = 14;
  const PROJECT_LIST_PATH_MODE_STORAGE_KEY = 'acliv:project-list-path-mode';
  const SESSION_ID_VISIBILITY_STORAGE_KEY = 'acliv:show-session-ids';
  let locale = $state<Locale>(getInitialLocale());

  function t(key: TranslationKey, params?: TranslationParams): string {
    return translate(locale, key, params);
  }

  function keyMessage(key: TranslationKey, params?: TranslationParams): UiMessage {
    return { kind: 'key', key, params };
  }

  function textMessage(text: string): UiMessage {
    return { kind: 'text', text };
  }

  function resolveMessage(message?: UiMessage | null): string {
    if (!message) return '';
    return message.kind === 'key' ? t(message.key, message.params) : message.text;
  }

  function uiMessageFromError(
    error: unknown,
    fallbackKey: TranslationKey = 'errors.request.internal_error',
  ): UiMessage {
    const key = api.getErrorTranslationKey(error);
    if (key) {
      return keyMessage(key);
    }

    if (error instanceof Error && error.message.trim()) {
      return textMessage(error.message);
    }

    return keyMessage(fallbackKey);
  }

  function updateLocale(nextLocale: Locale) {
    locale = nextLocale;
    persistLocale(nextLocale);
  }

  // ---- 适配函数 ----
  const GEMINI_GROUP = 'Gemini Sessions';
  const isWebMode = api.isWebMode();
  const PROVIDER_GROUPS: Record<string, string> = {
    gemini: GEMINI_GROUP,
    codex: 'Codex Sessions',
    openclaw: 'OpenClaw Sessions',
    opencode: 'OpenCode Sessions',
    claude: 'Claude Sessions',
  };

  function formatTimestamp(ms?: number): string {
    if (!ms) return t('common.na');
    return new Intl.DateTimeFormat(locale, {
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      second: '2-digit',
    }).format(new Date(ms));
  }
  function baseName(value?: string | null): string {
    if (!value?.trim()) return '';
    const normalized = value.trim().replace(/[\\/]+$/, '');
    const parts = normalized.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? normalized;
  }
  function defaultSessionTitle(projectDir?: string | null): string {
    return baseName(projectDir) || projectDir?.trim() || t('common.conversation');
  }
  function sessionTitle(s: SessionMeta): string {
    return s.title?.trim() || defaultSessionTitle(s.projectDir);
  }
  function indexedSessionTitle(item: IndexLibraryItem): string {
    return item.title?.trim()
      || item.projectName?.trim()
      || item.project?.trim()
      || t('common.conversation');
  }
  function sessionDir(s: SessionMeta): string {
    if (s.projectDir && s.projectDir.trim()) return s.projectDir;
    return PROVIDER_GROUPS[s.providerId] ?? 'Unknown';
  }
  function sessionCacheKey(s: SessionMeta): string {
    return `${s.providerId}:${s.sourcePath ?? s.sessionId}`;
  }
  function sessionMessageCount(s: SessionMeta): number {
    return messageCountCache[sessionCacheKey(s)] ?? 0;
  }
  function toSessionMeta(indexed: api.IndexedSession): SessionMeta {
    return {
      providerId: indexed.providerId,
      sessionId: indexed.sessionId,
      title: indexed.title,
      summary: indexed.summary,
      projectDir: indexed.project,
      cwd: indexed.cwd ?? indexed.project,
      model: indexed.model,
      createdAt: indexed.createdAt,
      lastActiveAt: indexed.lastActiveAt,
      sourcePath: indexed.sourcePath,
      resumeCommand: indexed.resumeCommand,
    };
  }

  // SessionMeta[] → ProjectInfo[]（按 source 过滤 + 按 projectDir 分组）
  function buildProjects(sessions: SessionMeta[], source: string): ProjectInfo[] {
    const map = new Map<string, { count: number; latest: number }>();
    sessions
      .filter(s => s.providerId === source)
      .forEach(s => {
        const dir = sessionDir(s);
        const e = map.get(dir) ?? { count: 0, latest: 0 };
        e.count++;
        if (s.lastActiveAt && s.lastActiveAt > e.latest) e.latest = s.lastActiveAt;
        map.set(dir, e);
      });
    return [...map.entries()].map(([name, { count, latest }]) => ({
      name, conversation_count: count, latest_date: formatTimestamp(latest),
    })).sort((a,b) => b.conversation_count - a.conversation_count);
  }

  // SessionMeta[] → ConvSummary[]（按 source + projectDir 过滤）
  function buildConversations(sessions: SessionMeta[], source: string, project: string): ConvSummary[] {
    return sessions
      .filter(s => s.providerId === source && sessionDir(s) === project)
      .map(s => ({
        session_id: s.sessionId, project_path: s.projectDir ?? '',
        source_type: s.providerId, title: sessionTitle(s),
        timestamp: formatTimestamp(s.lastActiveAt ?? s.createdAt),
        message_count: sessionMessageCount(s),
        date: formatTimestamp(s.lastActiveAt ?? s.createdAt),
      }));
  }

  // 从 allSessions 重新计算 projects / stats（切换 source 或刷新后调用）
  function refreshFromSessions() {
    const projs = buildProjects(allSessions, currentSource);
    projects = projs;
    stats = {
      projects_count: projs.length,
      conversations_count: allSessions.filter(s =>
        s.providerId === currentSource).length,
      messages_count: allSessions
        .filter(s => s.providerId === currentSource)
        .reduce((sum, s) => sum + sessionMessageCount(s), 0),
    };
    if (currentProject && !projs.some(project => project.name === currentProject)) {
      currentProject = null;
      currentConversation = null;
      conversations = [];
    }
    if (!currentProject && projs.length > 0) selectProject(projs[0].name, false);
  }

  // --- State (Svelte 5 Runes) ---
    let allSessions = $state<SessionMeta[]>([]);
  let messageCountCache = $state<Record<string, number>>({});
  let countJobToken = 0;
  let projects = $state<ProjectInfo[]>([]);
  let currentProject = $state<string | null>(null);
  let conversations = $state<ConvSummary[]>([]);
  let currentConversation = $state<any>(null);
  let stats = $state<Stats>({ projects_count: 0, conversations_count: 0, messages_count: 0 });
  let currentSource = $state(localStorage.getItem('source') || 'claude');
  const sources = ['claude', 'codex', 'gemini', 'openclaw', 'opencode'];

  // UI State
  let currentView = $state('list');
  let isSearchModalOpen = $state(false);
  let isIndexModalOpen = $state(false);
  let indexModalTab = $state<IndexModalTab>('overview');
  let searchQuery = $state('');
  let searchResults = $state<SearchResultLocal[]>([]);
  let searchIndexReady = $state(false);
  let searchIndexBootstrapping = $state(false);
  let searchIndexStatus = $state<api.SearchIndexStatus | null>(null);
  let indexLibraryItems = $state<IndexLibraryItem[]>([]);
  let indexProjectOptions = $state<api.IndexedProjectOption[]>([]);
  let indexLibraryTotalCount = $state(0);
  let indexLibraryPage = $state(1);
  let indexLibraryProviderFilter = $state('all');
  let indexLibraryProjectFilter = $state('all');
  let isIndexLibraryLoading = $state(false);
  let searchRequestToken = 0;
  let searchTimeRange = $state<SearchTimeRange>('all');
  let searchSort = $state<SearchSort>('relevance');
  let searchProjectOnly = $state(false);
  let searchTotalCount = $state(0);
  let activeSearchMatch = $state<{
      session_id: string;
      source_type: string;
      role: string;
      seq: number;
      query: string;
  } | null>(null);
  let hideFunctionCalls = $state(false);
  let compactProjectPaths = $state(localStorage.getItem(PROJECT_LIST_PATH_MODE_STORAGE_KEY) !== 'full');
  let showSessionIds = $state(localStorage.getItem(SESSION_ID_VISIBILITY_STORAGE_KEY) !== 'hidden');
  let isSourceDropdownOpen = $state(false);
  let theme = $state(localStorage.getItem('theme') || 'dark');
  let isLoading = $state(false);
  let isRefreshing = $state(false);
  let isDeleting = $state(false);
  let isIndexActionRunning = $state(false);
  let searchIndexSyncInfo = $state<{
      phase: string;
      message?: UiMessage;
      count: number;
      total: number;
  } | null>(null);
  let showToast = $state(false);
  let toastType = $state<'syncing' | 'success' | 'error'>('syncing');
  let toastMessage = $state<UiMessage>(keyMessage('toast.history_updated'));
  let MarkdownComponent = $state<MarkdownComponentType | null>(null);
  let deleteTarget = $state<SessionMeta | null>(null);
  let isProjectMenuOpen = $state(false);
  let isConversationRefreshing = $state(false);
  let authInitialized = $state(!isWebMode);
  let isAuthenticated = $state(!isWebMode);
  let loginUsername = $state('admin');
  let loginPassword = $state('');
  let loginError = $state<UiMessage | null>(null);
  let isLoggingIn = $state(false);

  // Timers
  let autoRefreshInterval: any;
  let searchTimer: any;
  let watcherReloadTimer: any;
  let markdownRendererPromise: Promise<MarkdownComponentType | null> | null = null;
  let searchIndexEventUnsubscribers: Array<() => void> = [];
  let watcherReloadRunning = false;
  let watcherReloadQueued = false;
  let pendingWatcherSources = new Map<string, api.IndexedSourceRef>();
  let conversationDetailElement = $state<HTMLDivElement | null>(null);
  let conversationHeaderElement = $state<HTMLDivElement | null>(null);
  let messagesContainerElement = $state<HTMLDivElement | null>(null);
  let conversationProgressAnchors = $state<ConversationProgressAnchor[]>([]);
  let activeConversationProgressKey = $state<string | null>(null);
  let isConversationProgressExpanded = $state(false);
  let conversationProgressResizeObserver: ResizeObserver | null = null;
  let conversationProgressFrame = 0;

  async function refreshWebAuthState(): Promise<boolean> {
    if (!isWebMode) return true;

    try {
      const session = await api.verifyWebAuth();
      authInitialized = true;
      isAuthenticated = true;
      loginUsername = session.username || loginUsername;
      loginError = null;
      return true;
    } catch (e) {
      api.clearWebToken();
      authInitialized = true;
      isAuthenticated = false;
      if (api.getErrorCode(e) !== 'auth.missing_token') {
        loginError = null;
      }
      return false;
    }
  }

  async function bootstrapAuthenticatedApp() {
    await loadData();
    await syncConversationFromRoute();
    void bootstrapSearchIndex();
  }

  function loadMarkdownRenderer(): Promise<MarkdownComponentType | null> {
    if (MarkdownComponent) {
      return Promise.resolve(MarkdownComponent);
    }
    if (!markdownRendererPromise) {
      markdownRendererPromise = import('./lib/components/Markdown.svelte')
        .then((module) => {
          MarkdownComponent = module.default;
          return module.default;
        })
        .catch((error) => {
          console.error('Failed to load Markdown renderer:', error);
          markdownRendererPromise = null;
          return null;
        });
    }
    return markdownRendererPromise;
  }

  async function handleLoginSubmit() {
    if (!isWebMode || isLoggingIn) return;

    const username = loginUsername.trim();
    const password = loginPassword.trim();
    if (!username || !password) {
      loginError = keyMessage('errors.auth.missing_credentials');
      return;
    }

    isLoggingIn = true;
    loginError = null;
    try {
      const result = await api.loginWeb(username, password);
      api.setWebToken(result.token);
      authInitialized = true;
      isAuthenticated = true;
      loginUsername = result.username;
      loginPassword = '';
      await bootstrapAuthenticatedApp();
    } catch (e) {
      api.clearWebToken();
      isAuthenticated = false;
      loginError = uiMessageFromError(e);
    } finally {
      isLoggingIn = false;
    }
  }

  function routeSessionId(): string | null {
    if (!isWebMode) return null;

    const pathname = decodeURIComponent(window.location.pathname || '/');
    const segments = pathname.split('/').filter(Boolean);
    if (segments.length !== 1) return null;

    return segments[0]?.trim() || null;
  }

  function updateConversationRoute(sessionId: string | null, mode: Exclude<RouteMode, 'none'> = 'push') {
    if (!isWebMode) return;

    const url = new URL(window.location.href);
    url.pathname = sessionId ? `/${encodeURIComponent(sessionId)}` : '/';
    const nextUrl = `${url.pathname}${url.search}${url.hash}`;
    const currentUrl = `${window.location.pathname}${window.location.search}${window.location.hash}`;
    if (nextUrl === currentUrl) return;

    if (mode === 'replace') {
      window.history.replaceState({}, '', nextUrl);
    } else {
      window.history.pushState({}, '', nextUrl);
    }
  }

  function detailScrollStorageKey(providerId: string, sessionId: string): string {
    return `${DETAIL_SCROLL_STORAGE_PREFIX}${providerId}:${sessionId}`;
  }

  function isScrolledNearBottom(element: HTMLElement): boolean {
    return element.scrollHeight - element.scrollTop - element.clientHeight <= DETAIL_SCROLL_BOTTOM_THRESHOLD;
  }

  function currentConversationTarget(): SessionMeta | null {
    return currentConversation
      ? getSessionById(currentConversation.session_id, currentConversation.source_type)
      : null;
  }

  function persistConversationScrollState(target: SessionMeta | null = currentConversationTarget()) {
    if (!target || !conversationDetailElement) return;

    const payload: DetailScrollState = {
      top: conversationDetailElement.scrollTop,
      atBottom: isScrolledNearBottom(conversationDetailElement),
    };

    try {
      sessionStorage.setItem(
        detailScrollStorageKey(target.providerId, target.sessionId),
        JSON.stringify(payload),
      );
    } catch (e) {
      console.error('Failed to persist conversation scroll state:', e);
    }
  }

  function readConversationScrollState(target: SessionMeta): DetailScrollState | null {
    try {
      const raw = sessionStorage.getItem(detailScrollStorageKey(target.providerId, target.sessionId));
      if (!raw) return null;

      const parsed = JSON.parse(raw) as Partial<DetailScrollState>;
      if (typeof parsed.top !== 'number' || typeof parsed.atBottom !== 'boolean') {
        return null;
      }

      return {
        top: parsed.top,
        atBottom: parsed.atBottom,
      };
    } catch (e) {
      console.error('Failed to read conversation scroll state:', e);
      return null;
    }
  }

  async function restoreConversationViewport(
    target: SessionMeta,
    options: { restoreScroll: boolean; scrollToBottom: boolean; highlightSearchMatch: boolean },
  ) {
    await tick();

    if (!conversationDetailElement) return;
    syncConversationProgressObserverTargets();

    if (options.highlightSearchMatch) {
      scrollActiveSearchMatchIntoView();
    } else if (options.scrollToBottom) {
      conversationDetailElement.scrollTo({
        top: conversationDetailElement.scrollHeight,
        behavior: 'auto',
      });
      persistConversationScrollState(target);
    } else if (options.restoreScroll) {
      const saved = readConversationScrollState(target);
      if (saved) {
        const maxScrollTop = Math.max(
          0,
          conversationDetailElement.scrollHeight - conversationDetailElement.clientHeight,
        );
        conversationDetailElement.scrollTop = saved.atBottom
          ? maxScrollTop
          : Math.min(Math.max(saved.top, 0), maxScrollTop);
      }
    }

    scheduleConversationProgressUpdate();
  }

  function handleConversationDetailScroll() {
    if (currentView !== 'detail') return;
    persistConversationScrollState();
    updateConversationProgressActiveAnchor();
  }

  function syncConversationContext(target: SessionMeta) {
    if (currentSource !== target.providerId) {
      currentSource = target.providerId;
      localStorage.setItem('source', target.providerId);
    }

    currentProject = sessionDir(target);
    refreshFromSessions();
    conversations = currentProject
      ? buildConversations(allSessions, currentSource, currentProject)
      : [];
  }

  function goToConversationList(routeMode: RouteMode = isWebMode ? 'push' : 'none') {
    persistConversationScrollState();
    isProjectMenuOpen = false;
    activeSearchMatch = null;
    clearConversationProgress();
    conversationProgressResizeObserver?.disconnect();
    currentView = 'list';

    if (routeMode !== 'none') {
      updateConversationRoute(null, routeMode);
    }
  }

  async function syncConversationFromRoute() {
    const sessionId = routeSessionId();
    if (!sessionId) {
      goToConversationList('none');
      return;
    }

    const target = getSessionById(sessionId);
    if (!target) {
      currentConversation = null;
      goToConversationList('replace');
      return;
    }

    await selectConversation(target.sessionId, target.providerId, null, {
      routeMode: 'none',
      restoreScroll: true,
    });
  }

  function handleBrowserPopState() {
    void syncConversationFromRoute();
  }

  async function subscribeSearchIndexEvents() {
    if (isWebMode) return;

    const unlisten = await listen<SearchIndexSyncEvent>('search-index-sync', async (event) => {
      const payload = event.payload;
      if (payload.phase === 'refreshing') {
        searchIndexSyncInfo = {
          phase: 'refreshing',
          message: keyMessage('index.sync_message.refreshing'),
          count: payload.count ?? 0,
          total: payload.total ?? 0,
        };
        return;
      }

      if (payload.phase === 'error') {
        searchIndexSyncInfo = null;
        toastType = 'error';
        toastMessage = payload.message ? textMessage(payload.message) : keyMessage('toast.index_refresh_failed');
        showToast = true;
        setTimeout(() => {
          showToast = false;
        }, 3000);
        return;
      }

      if (payload.phase === 'scanning' || payload.phase === 'syncing') {
        searchIndexSyncInfo = {
          phase: payload.phase,
          message: syncPhaseMessage(payload.phase) ?? (payload.message ? textMessage(payload.message) : undefined),
          count: payload.count ?? 0,
          total: payload.total ?? 0,
        };
        return;
      }

      if (payload.phase !== 'updated') return;

      searchIndexSyncInfo = {
        phase: 'done',
        message: keyMessage('toast.index_current'),
        count: payload.count ?? 0,
        total: payload.total ?? 0,
      };
      if (payload.status) {
        searchIndexStatus = payload.status;
        searchIndexReady = payload.status.ready && payload.status.sessionsCount > 0;
      } else {
        searchIndexReady = (payload.indexedSessions ?? 0) > 0;
      }
      const changedCount =
        (payload.addedSessions ?? 0) +
        (payload.updatedSessions ?? 0) +
        (payload.removedSessions ?? 0);

      if (changedCount > 0 || (payload.errorSessions ?? 0) > 0) {
        scheduleWatcherReload(payload.changedSources ?? []);
      }
    });

    searchIndexEventUnsubscribers = [unlisten];
  }

  async function refreshSearchIndexStatus() {
    try {
      searchIndexStatus = await api.getSearchIndexStatus();
      searchIndexReady = searchIndexStatus.ready && searchIndexStatus.sessionsCount > 0;
    } catch (e) {
      console.error('Failed to refresh search index status:', e);
    }
  }

  function getSearchSinceTs(range: SearchTimeRange): number | null {
    if (range === 'all') return null;

    const days = range === '7d' ? 7 : range === '30d' ? 30 : 90;
    return Date.now() - days * 24 * 60 * 60 * 1000;
  }

  function matchesSearchFilters(session: SessionMeta, query: string, source = currentSource): boolean {
    if (session.providerId !== source) return false;
    if (searchProjectOnly && currentProject && sessionDir(session) !== currentProject) return false;

    const sinceTs = getSearchSinceTs(searchTimeRange);
    const activeTs = session.lastActiveAt ?? session.createdAt ?? 0;
    if (sinceTs !== null && activeTs < sinceTs) return false;

    const needle = query.toLowerCase();
    return [session.sessionId, session.title, session.summary, session.projectDir]
      .some(field => field?.toLowerCase().includes(needle));
  }

  function watcherSourceKey(source: api.IndexedSourceRef): string {
    return `${source.providerId}:${source.sourcePath}`;
  }

  function mergeIndexedSessions(
    baseSessions: SessionMeta[],
    changedSources: api.IndexedSourceRef[],
    replacements: api.IndexedSession[],
  ): SessionMeta[] {
    const changedKeys = new Set(changedSources.map(watcherSourceKey));
    const replacementSessions = replacements.map(toSessionMeta);
    const nextCounts = { ...messageCountCache };

    for (const source of changedSources) {
      delete nextCounts[watcherSourceKey(source)];
    }
    for (const item of replacements) {
      nextCounts[`${item.providerId}:${item.sourcePath}`] = item.messageCount;
    }
    messageCountCache = nextCounts;

    return [...baseSessions.filter(session => {
      const sourcePath = session.sourcePath?.trim();
      if (!sourcePath) return true;
      return !changedKeys.has(`${session.providerId}:${sourcePath}`);
    }), ...replacementSessions].sort((a, b) =>
      (b.lastActiveAt ?? b.createdAt ?? 0) - (a.lastActiveAt ?? a.createdAt ?? 0),
    );
  }

  async function refreshChangedSessions(changedSources: api.IndexedSourceRef[]): Promise<boolean> {
    const uniqueSources = Array.from(
      new Map(
        changedSources
          .filter(source => source.providerId?.trim() && source.sourcePath?.trim())
          .map(source => [watcherSourceKey(source), source]),
      ).values(),
    );
    if (uniqueSources.length === 0) return false;

    const grouped = new Map<string, string[]>();
    for (const source of uniqueSources) {
      const paths = grouped.get(source.providerId) ?? [];
      paths.push(source.sourcePath);
      grouped.set(source.providerId, paths);
    }

    const replacements: api.IndexedSession[] = [];
    for (const [providerId, sourcePaths] of grouped) {
      const items = await api.listIndexedSessionsBySourcePaths(providerId, sourcePaths);
      replacements.push(...items);
    }

    const currentSession = currentConversation
      ? getSessionById(currentConversation.session_id, currentConversation.source_type)
      : null;
    const currentSessionKey = currentSession?.sourcePath
      ? `${currentSession.providerId}:${currentSession.sourcePath}`
      : null;
    const changedKeys = new Set(uniqueSources.map(watcherSourceKey));
    const replacementKeys = new Set(replacements.map(item => `${item.providerId}:${item.sourcePath}`));

    const nextSessions = mergeIndexedSessions(allSessions, uniqueSources, replacements);
    applyLoadedSessions(nextSessions);

    if (currentSessionKey && changedKeys.has(currentSessionKey)) {
      if (replacementKeys.has(currentSessionKey) && currentConversation) {
        await selectConversation(currentConversation.session_id, currentConversation.source_type, null, {
          routeMode: 'none',
          restoreScroll: true,
        });
      } else {
        currentConversation = null;
        goToConversationList('replace');
      }
    }

    return true;
  }

  async function runSearchIndexAction(kind: 'refresh' | 'rebuild') {
    if (isIndexActionRunning) return;

    isIndexActionRunning = true;
    searchIndexSyncInfo = {
      phase: kind === 'refresh' ? 'syncing' : 'scanning',
      message: kind === 'refresh' ? keyMessage('toast.refreshing_index') : keyMessage('toast.rebuilding_index'),
      count: 0,
      total: 0,
    };
    toastType = 'syncing';
    toastMessage = kind === 'refresh' ? keyMessage('toast.refreshing_index') : keyMessage('toast.rebuilding_index');
    showToast = true;

    try {
      if (kind === 'refresh') {
        const result = await api.refreshSearchIndex();
        searchIndexReady = result.indexedSessions > 0;
      } else {
        const result = await api.rebuildSearchIndex();
        searchIndexReady = result.indexedSessions > 0;
      }

        await refreshSearchIndexStatus();
        const sessions = await loadSessionInventory(searchIndexReady);
        applyLoadedSessions(sessions);
        if (isIndexModalOpen && indexModalTab === 'sessions') {
          await refreshIndexLibrary(false, true);
        }
        if (currentProject) void warmupMessageCounts(currentProject);

      toastType = 'success';
      toastMessage = kind === 'refresh' ? keyMessage('toast.index_refreshed') : keyMessage('toast.index_rebuilt');
      searchIndexSyncInfo = {
        phase: 'done',
        message: kind === 'refresh' ? keyMessage('toast.index_refreshed') : keyMessage('toast.index_rebuilt'),
        count: searchIndexStatus?.sessionsCount ?? 0,
        total: searchIndexStatus?.sessionsCount ?? 0,
      };
    } catch (e) {
      console.error(`Failed to ${kind} search index:`, e);
      searchIndexSyncInfo = null;
      toastType = 'error';
      toastMessage = kind === 'refresh' ? keyMessage('toast.index_refresh_failed') : keyMessage('toast.index_rebuild_failed');
    } finally {
      showToast = true;
      setTimeout(() => {
        showToast = false;
      }, 2500);
      isIndexActionRunning = false;
    }
  }

  onMount(async () => {
    setTheme(theme);
    void loadMarkdownRenderer();
    if (isWebMode) {
      await refreshWebAuthState();
      if (isAuthenticated) {
        await bootstrapAuthenticatedApp();
      }
    } else {
      await bootstrapAuthenticatedApp();
    }
    await subscribeSearchIndexEvents();
    autoRefreshInterval = setInterval(
      silentRefresh,
      isWebMode ? AUTO_SYNC_INTERVAL_WEB_MS : AUTO_SYNC_INTERVAL_DESKTOP_MS,
    );
    window.addEventListener('keydown', handleGlobalKeydown);
    window.addEventListener('click', handleWindowClick);
    if (isWebMode) {
      window.addEventListener('popstate', handleBrowserPopState);
    }
  });

  onDestroy(() => {
    if (autoRefreshInterval) clearInterval(autoRefreshInterval);
    if (searchTimer) clearTimeout(searchTimer);
    if (watcherReloadTimer) clearTimeout(watcherReloadTimer);
    if (conversationProgressFrame) cancelAnimationFrame(conversationProgressFrame);
    conversationProgressResizeObserver?.disconnect();
    countJobToken++;
    for (const unlisten of searchIndexEventUnsubscribers) {
      unlisten();
    }
    searchIndexEventUnsubscribers = [];
    window.removeEventListener('keydown', handleGlobalKeydown);
    window.removeEventListener('click', handleWindowClick);
    if (isWebMode) {
      window.removeEventListener('popstate', handleBrowserPopState);
    }
  });

  function scheduleWatcherReload(changedSources: api.IndexedSourceRef[] = []) {
    for (const source of changedSources) {
      if (source.providerId?.trim() && source.sourcePath?.trim()) {
        pendingWatcherSources.set(watcherSourceKey(source), source);
      }
    }
    if (watcherReloadTimer) clearTimeout(watcherReloadTimer);
    watcherReloadTimer = setTimeout(() => {
      void flushWatcherReload();
    }, 1800);
  }

  async function flushWatcherReload() {
    if (watcherReloadRunning) {
      watcherReloadQueued = true;
      return;
    }

    watcherReloadRunning = true;
    try {
      const pendingSources = Array.from(pendingWatcherSources.values());
      pendingWatcherSources.clear();

      let appliedIncremental = false;
      if (pendingSources.length > 0 && pendingSources.length <= 200) {
        try {
          appliedIncremental = await refreshChangedSessions(pendingSources);
        } catch (e) {
          console.error('Incremental indexed session refresh failed, falling back to full reload:', e);
        }
      }

      if (!appliedIncremental) {
        if (!searchIndexStatus) {
          await refreshSearchIndexStatus();
        }
        const sessions = await loadSessionInventory(searchIndexReady);
        applyLoadedSessions(sessions);
      }

        if (isIndexModalOpen && indexModalTab === 'sessions') {
          await refreshIndexLibrary(false, true);
        }
        if (currentProject) void warmupMessageCounts(currentProject);
    } catch (e) {
      console.error('Failed to reload sessions after watcher refresh:', e);
    } finally {
      watcherReloadRunning = false;
      if (watcherReloadQueued) {
        watcherReloadQueued = false;
        scheduleWatcherReload();
      }
    }
  }
  async function loadIndexedSessions(): Promise<SessionMeta[]> {
    const indexed = await api.listIndexedSessions(null, 5000);
    const nextCounts = { ...messageCountCache };
    const sessions = indexed.map(item => {
      const session = toSessionMeta(item);
      nextCounts[sessionCacheKey(session)] = item.messageCount;
      return session;
    });
    messageCountCache = nextCounts;
    return sessions;
  }

  function currentIndexProviderFilter(): string | null {
    return indexLibraryProviderFilter === 'all' ? null : indexLibraryProviderFilter;
  }

  function currentIndexProjectFilter(): string | null {
    return indexLibraryProjectFilter === 'all' ? null : indexLibraryProjectFilter;
  }

  async function refreshIndexProjects() {
    try {
      indexProjectOptions = await api.listIndexedProjects(currentIndexProviderFilter());
      if (
        indexLibraryProjectFilter !== 'all' &&
        !indexProjectOptions.some(project => project.project === indexLibraryProjectFilter)
      ) {
        indexLibraryProjectFilter = 'all';
      }
    } catch (e) {
      console.error('Failed to load indexed projects:', e);
      indexProjectOptions = [];
      indexLibraryProjectFilter = 'all';
    }
  }

  async function refreshIndexLibrary(resetPage = false, reloadProjects = false) {
    if (reloadProjects) {
      await refreshIndexProjects();
    }

    isIndexLibraryLoading = true;
    try {
      let nextPage = resetPage ? 1 : indexLibraryPage;

      while (true) {
        const offset = Math.max(0, (nextPage - 1) * INDEX_LIBRARY_PAGE_SIZE);
        const result = await api.listIndexedSessionsPage(
          currentIndexProviderFilter(),
          currentIndexProjectFilter(),
          INDEX_LIBRARY_PAGE_SIZE,
          offset,
        );
        const pageCount = Math.max(1, Math.ceil(result.totalCount / INDEX_LIBRARY_PAGE_SIZE));

        if (result.totalCount > 0 && nextPage > pageCount) {
          nextPage = pageCount;
          continue;
        }

        indexLibraryPage = nextPage;
        indexLibraryTotalCount = result.totalCount;
        indexLibraryItems = result.items;
        break;
      }
    } catch (e) {
      console.error('Failed to load indexed sessions:', e);
      indexLibraryTotalCount = 0;
      indexLibraryItems = [];
    } finally {
      isIndexLibraryLoading = false;
    }
  }

  async function loadSessionInventory(preferIndexed = searchIndexReady): Promise<SessionMeta[]> {
    if (preferIndexed) {
      try {
        return await loadIndexedSessions();
      } catch (e) {
        console.error('Indexed session list failed, falling back to source scan:', e);
        searchIndexReady = false;
      }
    }

    const sessions = await api.listSessions();
    return sessions;
  }

  function applyLoadedSessions(sessions: SessionMeta[]) {
    allSessions = sessions;
    refreshFromSessions();
    if (currentProject) {
      conversations = buildConversations(allSessions, currentSource, currentProject);
    }
  }

  async function warmupMessageCounts(projectName: string) {
    const token = ++countJobToken;
    const targets = allSessions
      .filter(s => s.providerId === currentSource && sessionDir(s) === projectName && !!s.sourcePath)
      .filter(s => messageCountCache[sessionCacheKey(s)] === undefined);
    if (!targets.length) return;

    const LIMIT = 40;
    const BATCH_SIZE = 6;
    const subset = targets.slice(0, LIMIT);
    for (let i = 0; i < subset.length; i += BATCH_SIZE) {
      if (token !== countJobToken) return;
      const batch = subset.slice(i, i + BATCH_SIZE);
      const results = await Promise.all(batch.map(async (s) => {
        try {
          const list = await loadConversationMessages(s);
          return { key: sessionCacheKey(s), count: list.length };
        } catch {
          return { key: sessionCacheKey(s), count: 0 };
        }
      }));
      if (token !== countJobToken) return;
      const next = { ...messageCountCache };
      for (const r of results) next[r.key] = r.count;
      messageCountCache = next;
      refreshFromSessions();
      if (currentProject) {
        conversations = buildConversations(allSessions, currentSource, currentProject);
      }
    }
  }
  async function loadData() {
    isLoading = true;
    try {
        await refreshSearchIndexStatus();
        const sessions = await loadSessionInventory(searchIndexReady);
        applyLoadedSessions(sessions);
    } catch (e) {
      if (handleWebUnauthorized(e)) {
          return;
      }
      console.error("Failed to load data:", e);
      showFeedback(uiMessageFromError(e), 'error');
    } finally {
        isLoading = false;
    }
  }
  async function silentRefresh() {
      if (isWebMode && !isAuthenticated) return;
      if (isLoading || isRefreshing) return;
      isRefreshing = true;
      toastType = 'syncing';
      toastMessage = keyMessage('toast.syncing_history');
      showToast = true;
      
      try {
          if (searchIndexReady) {
              try {
                  const result = await api.refreshSearchIndex();
                  searchIndexReady = result.indexedSessions > 0;
              } catch (indexError) {
                  console.error('Search index refresh failed:', indexError);
                  searchIndexReady = false;
              }
          }
          await refreshSearchIndexStatus();
          const sessions = await loadSessionInventory(searchIndexReady);
          applyLoadedSessions(sessions);
          if (currentProject) void warmupMessageCounts(currentProject);
          showToast = false;
          isRefreshing = false;
      } catch(e) { 
          if (handleWebUnauthorized(e)) {
              showToast = false;
              isRefreshing = false;
              return;
          }
          console.error("Silent refresh failed:", e); 
          showToast = false;
          isRefreshing = false;
      }
  }
  function getSessionById(sessionId: string, sourceType?: string): SessionMeta | null {
      return allSessions.find(s =>
          s.sessionId === sessionId && (!sourceType || s.providerId === sourceType)) ?? null;
  }

  function openDeleteDialog() {
      const target = currentConversation
          ? getSessionById(currentConversation.session_id, currentConversation.source_type)
          : null;
      if (!target?.sourcePath || isDeleting) return;
      deleteTarget = target;
  }

  function closeDeleteDialog() {
      if (isDeleting) return;
      deleteTarget = null;
  }

  async function confirmDeleteSession() {
      if (!deleteTarget?.sourcePath || isDeleting) return;

      isDeleting = true;
      try {
          await api.deleteSession({
              providerId: deleteTarget.providerId,
              sessionId: deleteTarget.sessionId,
              sourcePath: deleteTarget.sourcePath,
          });

          const deletedProject = sessionDir(deleteTarget);
          deleteTarget = null;
          currentConversation = null;
          goToConversationList('replace');

          const sessions = await loadSessionInventory(searchIndexReady);
          await refreshSearchIndexStatus();
          applyLoadedSessions(sessions);

          const projectStillExists = sessions.some(
              s => s.providerId === currentSource && sessionDir(s) === deletedProject,
          );
          currentProject = projectStillExists ? deletedProject : null;
          conversations = currentProject ? buildConversations(sessions, currentSource, currentProject) : [];
          if (currentProject) void warmupMessageCounts(currentProject);

          toastType = 'success';
          toastMessage = keyMessage('toast.session_deleted');
          showToast = true;
          setTimeout(() => {
              showToast = false;
          }, 3000);
      } catch (e) {
          console.error('Failed to delete session:', e);
          toastType = 'error';
          toastMessage = keyMessage('toast.delete_failed');
          showToast = true;
          setTimeout(() => {
              showToast = false;
          }, 3000);
      } finally {
          isDeleting = false;
      }
  }
  function selectProject(name: string, clearRoute = true) {
    currentProject = name;
    conversations = buildConversations(allSessions, currentSource, name);
    if (clearRoute) {
      goToConversationList('replace');
    } else {
      currentView = 'list';
      activeSearchMatch = null;
    }
    void warmupMessageCounts(name);
  }

  function showFeedback(message: string | UiMessage, type: 'success' | 'error' | 'syncing' = 'success') {
      toastType = type;
      toastMessage = typeof message === 'string' ? textMessage(message) : message;
      showToast = true;
      setTimeout(() => {
          showToast = false;
      }, 2500);
  }

  function handleWebUnauthorized(error: unknown): boolean {
      if (!isWebMode) return false;

      const message = error instanceof Error ? error.message : String(error);
      const code = api.getErrorCode(error);
      if (code !== 'auth.missing_token' && message !== 'Unauthorized' && message !== 'Missing web token. Login required.') {
          return false;
      }

      api.clearWebToken();
      authInitialized = true;
      isAuthenticated = false;
      loginPassword = '';
      loginError = keyMessage('errors.auth.session_expired');
      currentConversation = null;
      currentView = 'list';
      return true;
  }

  async function copyText(text: string, message: UiMessage) {
      try {
          await navigator.clipboard.writeText(text);
          showFeedback(message, 'success');
      } catch (e) {
          console.error('Copy failed:', e);
          showFeedback(keyMessage('toast.copy_failed'), 'error');
      }
  }

  async function openResumeTerminal(kind: 'cmd' | 'powershell') {
      const target = currentConversation
          ? getSessionById(currentConversation.session_id, currentConversation.source_type)
          : null;
      if (!target?.resumeCommand) return;

      if (isWebMode) {
          await copyText(target.resumeCommand, keyMessage('toast.resume_command_copied'));
          return;
      }

      try {
          if (kind === 'powershell') {
              try {
                  await navigator.clipboard.writeText(target.resumeCommand);
              } catch (copyError) {
                  console.error('Copy failed before launching PowerShell:', copyError);
              }
          }
          await api.launchTerminal(target.resumeCommand, target.projectDir, kind);
          showFeedback(
              kind === 'cmd'
                  ? keyMessage('toast.opened_in_cmd')
                  : keyMessage('toast.opened_in_powershell'),
              'success',
          );
      } catch (e) {
          console.error('Launch terminal failed:', e);
          showFeedback(keyMessage('toast.launch_terminal_failed'), 'error');
      }
  }

  function toggleProjectMenu(event: MouseEvent) {
      event.stopPropagation();
      isProjectMenuOpen = !isProjectMenuOpen;
  }

  async function handleProjectPathCopy(event?: MouseEvent) {
      event?.stopPropagation();
      if (!selectedSession?.projectDir) return;
      isProjectMenuOpen = false;
      await copyText(selectedSession.projectDir, keyMessage('toast.project_path_copied'));
  }

  async function openProjectInExplorer(event?: MouseEvent) {
      event?.stopPropagation();
      if (isWebMode) {
          showFeedback(keyMessage('errors.feature.web_only'), 'error');
          return;
      }
      const target = currentConversation
          ? getSessionById(currentConversation.session_id, currentConversation.source_type)
          : null;
      if (!target?.projectDir) return;
      isProjectMenuOpen = false;

      try {
          await api.openInFileExplorer(target.projectDir);
          showFeedback(keyMessage('toast.opened_in_explorer'), 'success');
      } catch (e) {
          console.error('Open in File Explorer failed:', e);
          showFeedback(keyMessage('toast.open_explorer_failed'), 'error');
      }
  }
  function mergeMessageContent(current: string, next: string) {
      if (!current) return next;
      if (!next) return current;
      const needsSingleNewline = current.endsWith('\n') || next.startsWith('\n');
      return `${current}${needsSingleNewline ? '\n' : '\n\n'}${next}`;
  }

  function normalizeMessageKind(message: ConversationMessageView): string {
      return message.kind?.trim() || 'message';
  }

  function canMergeMessageBlock(current: MessageBlock, next: ConversationMessageView): boolean {
      return current.kind === 'message'
          && normalizeMessageKind(next) === 'message'
          && current.role.toLowerCase() === (next.role || '').toLowerCase();
  }

  function buildMessageBlocks(messages: ConversationMessageView[]): MessageBlock[] {
      const blocks: MessageBlock[] = [];

      for (const message of messages) {
          const role = message.role || 'unknown';
          const kind = normalizeMessageKind(message);
          const content = message.content || '';
          const timestamp = message.timestamp || '';
          const seq = message.seq;
          const last = blocks[blocks.length - 1];

          if (last && canMergeMessageBlock(last, message)) {
              last.content = mergeMessageContent(last.content, content);
              last.timestamp = timestamp || last.timestamp;
              last.seqEnd = seq;
              continue;
          }

          blocks.push({
              role,
              kind,
              name: message.name,
              callId: message.callId,
              content,
              timestamp,
              seqStart: seq,
              seqEnd: seq,
          });
      }

      return blocks;
  }

  function buildToolGroupBlock(toolBlocks: MessageBlock[]): MessageBlock {
      const runBlocks = toolBlocks.filter(block => block.kind === 'function_call' || block.kind === 'tool_use');

      return {
          role: 'assistant',
          kind: 'tool_group',
          content: '',
          timestamp: toolBlocks[toolBlocks.length - 1]?.timestamp || toolBlocks[0]?.timestamp,
          seqStart: toolBlocks[0]?.seqStart ?? 0,
          seqEnd: toolBlocks[toolBlocks.length - 1]?.seqEnd ?? toolBlocks[0]?.seqStart ?? 0,
          blocks: toolBlocks,
          runCount: runBlocks.length || toolBlocks.length,
      };
  }

  function groupToolCallBlocks(blocks: MessageBlock[]): MessageBlock[] {
      const grouped: MessageBlock[] = [];

      for (let index = 0; index < blocks.length; index += 1) {
          const current = blocks[index];
          if (!isFunctionCallLikeBlock(current)) {
              grouped.push(current);
              continue;
          }

          const toolBlocks = [current];
          while (index + 1 < blocks.length && isFunctionCallLikeBlock(blocks[index + 1])) {
              toolBlocks.push(blocks[index + 1]);
              index += 1;
          }

          grouped.push(buildToolGroupBlock(toolBlocks));
      }

      return grouped;
  }

  function transformConversation(conv: ConversationLike | null) {
      if (!conv) return null;
      const messages = conv.messages || [];
      return { ...conv, blocks: buildMessageBlocks(messages) };
  }
  function isFunctionCallLikeBlock(block: MessageBlock): boolean {
      const kind = (block.kind || '').trim().toLowerCase();
      return kind === 'function_call'
          || kind === 'function_call_output'
          || kind === 'tool_use'
          || kind === 'tool_result';
  }
  function getVisibleConversationBlocks(blocks: MessageBlock[]): MessageBlock[] {
      if (hideFunctionCalls) {
          return blocks.filter(block => !isFunctionCallLikeBlock(block));
      }

      return groupToolCallBlocks(blocks);
  }
  function isBlockSearchMatch(block: MessageBlock): boolean {
      if (!activeSearchMatch) return false;
      const blockRole = block.role.toLowerCase();
      if (block.kind !== 'message') return false;
      if (blockRole !== activeSearchMatch.role && !(blockRole === 'human' && activeSearchMatch.role === 'user')) {
          return false;
      }
      return activeSearchMatch.seq >= block.seqStart && activeSearchMatch.seq <= block.seqEnd;
  }

  function getMessageBlockLabel(block: MessageBlock): string {
      const role = block.role.toLowerCase();
      if (isInstructionContextBlock(block)) return t('detail.startup_instructions');
      if (block.kind === 'tool_group') return getToolGroupSummary(block);
      if (block.kind !== 'message') return block.kind;
      if (role === 'assistant') return t('detail.assistant');
      if (role === 'developer') return t('detail.developer');
      if (role === 'human' || role === 'user') return t('detail.user');
      if (role === 'tool') return t('detail.tool');
      return role ? role.charAt(0).toUpperCase() + role.slice(1) : t('detail.message');
  }

  function getMessageBlockClass(block: MessageBlock): string {
      if (isInstructionContextBlock(block)) return 'instruction-message';
      if (block.kind === 'tool_group') return 'tool-group-message';
      if (block.kind === 'function_call' || block.kind === 'tool_use') return 'tool-call-message';
      if (block.kind === 'function_call_output' || block.kind === 'tool_result') return 'tool-result-message';
      const role = block.role.toLowerCase();
      if (role === 'assistant') return 'assistant-message';
      if (role === 'developer') return 'developer-message';
      if (role === 'human' || role === 'user') return 'user-message';
      return 'tool-result-message';
  }

  function isInstructionContextBlock(block: MessageBlock): boolean {
      if (block.kind !== 'message') return false;
      const content = (block.content || '').trim();
      if (!content) return false;

      return content.startsWith('# AGENTS.md instructions')
          || content.startsWith('# CLAUDE.md instructions')
          || content.startsWith('# AGENT.md instructions')
          || content.startsWith('# INSTRUCTIONS')
          || (
              content.includes('<INSTRUCTIONS>')
              && (content.includes('## Skills') || content.includes('## Plugins') || content.includes('AGENTS.md'))
          );
  }

  function isConversationProgressBlock(block: MessageBlock): boolean {
      if (block.kind !== 'message' || isInstructionContextBlock(block)) return false;
      const role = block.role.toLowerCase();
      return (role === 'user' || role === 'human') && !!getConversationProgressText(block.content);
  }

  function getConversationProgressKey(block: MessageBlock): string {
      return `${block.role.toLowerCase()}:${block.seqStart}:${block.seqEnd}`;
  }

  function getConversationProgressText(content: string): string {
      const normalized = content
          .replace(/The user interrupted the previous turn on purpose\.[\s\S]*?verify current state before retrying\.\s*/gi, ' ')
          .replace(/<turn_aborted>[\s\S]*?<\/turn_aborted>/gi, ' ')
          .replace(/<turn_aborted>[\s\S]*$/gi, ' ')
          .replace(/<image\b[^>]*>[\s\S]*?<\/image>/gi, ' ')
          .replace(/<image\b[^>]*\/?>/gi, ' ')
          .replace(/\s+/g, ' ')
          .trim();

      if (!normalized) return '';
      if (/^the user interrupted the previous turn on purpose\b/i.test(normalized)) {
          return '';
      }

      return normalized;
  }

  function getConversationProgressPreview(block: MessageBlock): string {
      const normalized = getConversationProgressText(block.content);
      if (!normalized) return t('detail.empty_progress');
      return Array.from(normalized).slice(0, 15).join('');
  }

  function getConversationProgressLabel(block: MessageBlock): string {
      return getConversationProgressText(block.content) || t('detail.empty_progress');
  }

  function isCollapsibleBlock(block: MessageBlock): boolean {
      return block.kind !== 'message' || isInstructionContextBlock(block) || block.role.toLowerCase() === 'developer';
  }

  function isToolGroupBlock(block: MessageBlock): boolean {
      return block.kind === 'tool_group';
  }

  function isRawTextBlock(block: MessageBlock): boolean {
      const kind = (block.kind || '').trim().toLowerCase();
      return kind === 'function_call'
          || kind === 'function_call_output'
          || kind === 'tool_use'
          || kind === 'tool_result';
  }

  function getToolGroupSummary(block: MessageBlock): string {
      const runCount = block.runCount ?? block.blocks?.length ?? 0;
      return t('detail.command_runs', { count: runCount });
  }

  function getToolGroupChildren(block: MessageBlock): MessageBlock[] {
      return block.blocks ?? [];
  }

  function getToolDetailNumber(block: MessageBlock): string {
      if (block.seqStart === block.seqEnd) {
          return `#${block.seqStart + 1}`;
      }

      return `#${block.seqStart + 1}-${block.seqEnd + 1}`;
  }

  function scrollActiveSearchMatchIntoView() {
      setTimeout(() => {
          const el = document.querySelector('.message.search-hit');
          if (el instanceof HTMLElement) {
              el.scrollIntoView({ block: 'center', behavior: 'smooth' });
          }
      }, 80);
  }

  function clearConversationProgress() {
      conversationProgressAnchors = [];
      activeConversationProgressKey = null;
      isConversationProgressExpanded = false;
  }

  function scheduleConversationProgressUpdate() {
      if (conversationProgressFrame) return;
      conversationProgressFrame = requestAnimationFrame(() => {
          conversationProgressFrame = 0;
          updateConversationProgressAnchors();
      });
  }

  function syncConversationProgressObserverTargets() {
      if (typeof ResizeObserver === 'undefined') return;

      if (!conversationProgressResizeObserver) {
          conversationProgressResizeObserver = new ResizeObserver(() => {
              scheduleConversationProgressUpdate();
          });
      }

      conversationProgressResizeObserver.disconnect();
      if (conversationDetailElement) {
          conversationProgressResizeObserver.observe(conversationDetailElement);
      }
      if (conversationHeaderElement) {
          conversationProgressResizeObserver.observe(conversationHeaderElement);
      }
      if (messagesContainerElement) {
          conversationProgressResizeObserver.observe(messagesContainerElement);
      }
  }

  function getConversationAnchorOffset(element: HTMLElement): number {
      if (!conversationDetailElement) return 0;
      const containerRect = conversationDetailElement.getBoundingClientRect();
      const elementRect = element.getBoundingClientRect();
      return elementRect.top - containerRect.top + conversationDetailElement.scrollTop;
  }

  function updateConversationProgressActiveAnchor(anchors: ConversationProgressAnchor[] = conversationProgressAnchors) {
      if (!conversationDetailElement || anchors.length === 0) {
          activeConversationProgressKey = null;
          return;
      }

      const readingLine = conversationDetailElement.scrollTop
          + Math.min(conversationDetailElement.clientHeight * 0.25, 120);
      let activeKey = anchors[0].key;
      for (const anchor of anchors) {
          if (anchor.top <= readingLine) {
              activeKey = anchor.key;
              continue;
          }
          break;
      }
      activeConversationProgressKey = activeKey;
  }

  function updateConversationProgressAnchors() {
      if (currentView !== 'detail' || !currentConversation || !conversationDetailElement) {
          clearConversationProgress();
          return;
      }

      const elements = Array.from(
          conversationDetailElement.querySelectorAll<HTMLElement>('[data-progress-anchor="true"]'),
      );
      if (elements.length === 0) {
          clearConversationProgress();
          return;
      }

      const anchors = elements.map((element, index) => {
          const top = getConversationAnchorOffset(element);
          return {
              key: element.dataset.progressKey || `anchor-${index}`,
              label: element.dataset.progressLabel || element.dataset.progressPreview || t('detail.user_message'),
              preview: element.dataset.progressPreview || t('detail.user_message'),
              top,
          };
      });

      conversationProgressAnchors = anchors;
      updateConversationProgressActiveAnchor(anchors);
  }

  function jumpToConversationProgressAnchor(anchor: ConversationProgressAnchor) {
      if (!conversationDetailElement) return;

      activeConversationProgressKey = anchor.key;
      conversationDetailElement.scrollTo({
          top: Math.max(anchor.top - 16, 0),
          behavior: 'smooth',
      });
  }

  function getConversationProgressNavHeight(count: number): string {
      const height = DETAIL_PROGRESS_ANCHOR_OFFSET * 2
          + Math.max(count - 1, 0) * DETAIL_PROGRESS_ANCHOR_SPACING
          + 8;
      return `${height}px`;
  }

  function getConversationProgressAnchorTop(index: number): string {
      return `${DETAIL_PROGRESS_ANCHOR_OFFSET + index * DETAIL_PROGRESS_ANCHOR_SPACING}px`;
  }

  function openConversationProgressDirectory() {
      isConversationProgressExpanded = true;
  }

  function closeConversationProgressDirectory() {
      isConversationProgressExpanded = false;
  }

  async function loadIndexedConversationMessages(target: SessionMeta): Promise<ConversationMessage[]> {
      if (!target.sourcePath) return [];
      const indexed = await api.getIndexedSessionMessages(target.providerId, target.sourcePath);
      return indexed.map(msg => ({
          role: msg.role,
          kind: msg.kind,
          name: msg.name,
          callId: msg.callId,
          content: msg.content,
          ts: msg.ts,
          seq: msg.seq,
      }));
  }

  async function loadConversationMessages(target: SessionMeta): Promise<ConversationMessage[]> {
      if (!target.sourcePath) return [];

      if (searchIndexReady) {
          try {
              return await loadIndexedConversationMessages(target);
          } catch (e) {
              console.error('Indexed message read failed, retrying indexed session refresh:', e);
              try {
                  await refreshChangedSessions([{
                      providerId: target.providerId,
                      sourcePath: target.sourcePath,
                  }]);
                  return await loadIndexedConversationMessages(target);
              } catch (retryError) {
                  console.error('Indexed message retry failed, falling back to source file:', retryError);
              }
          }
      }

      const raw = await api.getSessionMessages(target.providerId, target.sourcePath);
      return raw.map((msg, index) => ({
          role: msg.role,
          kind: msg.kind,
          name: msg.name,
          callId: msg.callId,
          content: msg.content,
          ts: msg.ts,
          seq: index,
      }));
  }

  async function selectConversation(
      sessionId: string,
      sourceType?: string,
      searchMatch?: SearchResultLocal | null,
      options: SelectConversationOptions = {},
  ) {
      const target = allSessions.find(s => s.sessionId === sessionId && (!sourceType || s.providerId === sourceType));
      if (!target) return;

      syncConversationContext(target);

      const rawMsgs = await loadConversationMessages(target);
      messageCountCache = {
        ...messageCountCache,
        [sessionCacheKey(target)]: rawMsgs.length,
      };
      const convLike = {
        session_id: target.sessionId,
        project_path: target.projectDir ?? '',
        source_type: target.providerId,
        title: sessionTitle(target),
        timestamp: formatTimestamp(target.lastActiveAt ?? target.createdAt),
        messages: rawMsgs.map(m => ({
          role: m.role,
          kind: m.kind,
          name: m.name,
          callId: m.callId,
          content: m.content,
          timestamp: formatTimestamp(m.ts),
          seq: m.seq,
        })),
      };
      void loadMarkdownRenderer();
      currentConversation = transformConversation(convLike as any);
      const matchRole = searchMatch?.match_role?.toLowerCase();
      const canHighlight = (matchRole === 'assistant' || matchRole === 'user' || matchRole === 'human')
          && searchMatch?.match_seq !== undefined;
      activeSearchMatch = canHighlight
          ? {
              session_id: target.sessionId,
              source_type: target.providerId,
              role: matchRole === 'assistant' ? 'assistant' : 'user',
              seq: searchMatch!.match_seq!,
              query: searchMatch.search_query ?? searchQuery,
          }
          : null;
      isProjectMenuOpen = false;
      currentView = 'detail';

      const routeMode = options.routeMode ?? (isWebMode ? 'push' : 'none');
      if (routeMode !== 'none') {
          updateConversationRoute(target.sessionId, routeMode);
      }

      await restoreConversationViewport(target, {
          restoreScroll: options.restoreScroll ?? !canHighlight,
          scrollToBottom: options.scrollToBottom ?? false,
          highlightSearchMatch: canHighlight,
      });
  }

  async function refreshCurrentConversation() {
      const currentTarget = currentConversationTarget();
      if (!currentTarget?.sourcePath || isConversationRefreshing) return;

      persistConversationScrollState(currentTarget);
      const keepBottomPinned = conversationDetailElement
          ? isScrolledNearBottom(conversationDetailElement)
          : false;

      isConversationRefreshing = true;
      toastType = 'syncing';
      toastMessage = keyMessage('toast.conversation_refreshing');
      showToast = true;
      let abortedForUnauthorized = false;

      try {
          const raw = await api.getSessionMessages(currentTarget.providerId, currentTarget.sourcePath);
          const latestMessages: ConversationMessage[] = raw.map((msg, index) => ({
              role: msg.role,
              kind: msg.kind,
              name: msg.name,
              callId: msg.callId,
              content: msg.content,
              ts: msg.ts,
              seq: index,
          }));

          messageCountCache = {
              ...messageCountCache,
              [sessionCacheKey(currentTarget)]: latestMessages.length,
          };

          const latestTimestamp = latestMessages.length > 0
              ? latestMessages[latestMessages.length - 1].ts
              : undefined;

          allSessions = allSessions
              .map(session =>
                  session.sessionId === currentTarget.sessionId && session.providerId === currentTarget.providerId
                      ? {
                          ...session,
                          lastActiveAt: latestTimestamp ?? session.lastActiveAt,
                        }
                      : session,
              )
              .sort((a, b) => (b.lastActiveAt ?? b.createdAt ?? 0) - (a.lastActiveAt ?? a.createdAt ?? 0));

          const convLike = {
              session_id: currentTarget.sessionId,
              project_path: currentTarget.projectDir ?? '',
              source_type: currentTarget.providerId,
              title: sessionTitle(currentTarget),
              timestamp: formatTimestamp(latestTimestamp ?? currentTarget.lastActiveAt ?? currentTarget.createdAt),
              messages: latestMessages.map(message => ({
                  role: message.role,
                  kind: message.kind,
                  name: message.name,
                  callId: message.callId,
                  content: message.content,
                  timestamp: formatTimestamp(message.ts),
                  seq: message.seq,
              })),
          };

          currentConversation = transformConversation(convLike as any);
          conversations = currentProject
              ? buildConversations(allSessions, currentSource, currentProject)
              : conversations;
          stats = {
              projects_count: projects.length,
              conversations_count: allSessions.filter(s => s.providerId === currentSource).length,
              messages_count: allSessions
                  .filter(s => s.providerId === currentSource)
                  .reduce((sum, s) => sum + sessionMessageCount(s), 0),
          };

          await restoreConversationViewport(currentTarget, {
              restoreScroll: !keepBottomPinned,
              scrollToBottom: keepBottomPinned,
              highlightSearchMatch: false,
          });

          toastType = 'success';
          toastMessage = keyMessage('toast.conversation_refreshed');
      } catch (e) {
          if (handleWebUnauthorized(e)) {
              abortedForUnauthorized = true;
              showToast = false;
              return;
          }
          console.error('Failed to refresh conversation:', e);
          toastType = 'error';
          toastMessage = keyMessage('toast.conversation_refresh_failed');
      } finally {
          if (!abortedForUnauthorized) {
              showToast = true;
              setTimeout(() => {
                  showToast = false;
              }, 2500);
          }
          isConversationRefreshing = false;
      }
  }

  function setTheme(newTheme: string) {
      theme = newTheme;
      document.documentElement.setAttribute('data-theme', theme);
      localStorage.setItem('theme', theme);
  }

  function toggleTheme() {
      setTheme(theme === 'dark' ? 'light' : 'dark');
  }
  function toggleHideFunctionCalls() {
      hideFunctionCalls = !hideFunctionCalls;
  }
  function toggleCompactProjectPaths() {
      compactProjectPaths = !compactProjectPaths;
      localStorage.setItem(PROJECT_LIST_PATH_MODE_STORAGE_KEY, compactProjectPaths ? 'compact' : 'full');
  }
  function toggleShowSessionIds() {
      showSessionIds = !showSessionIds;
      localStorage.setItem(SESSION_ID_VISIBILITY_STORAGE_KEY, showSessionIds ? 'shown' : 'hidden');
  }
  function selectSource(source: string) {
      if (currentSource === source) {
          isSourceDropdownOpen = false;
          return;
      }
      currentSource = source;
      localStorage.setItem('source', source);
      isSourceDropdownOpen = false;
      isProjectMenuOpen = false;
      currentProject = null;
      currentConversation = null;
      goToConversationList('replace');
      refreshFromSessions();
  }
  function buildMetadataSearchResults(query: string, source = currentSource): SearchResultLocal[] {
      const results = allSessions
        .filter(s => matchesSearchFilters(s, query, source))
        .map(s => ({
          project: sessionDir(s),
          source_type: s.providerId,
          session_id: s.sessionId,
          title: sessionTitle(s),
          date: formatTimestamp(s.lastActiveAt ?? s.createdAt),
          sort_ts: s.lastActiveAt ?? s.createdAt ?? 0,
        }))
        .sort((a, b) => searchSort === 'recent'
          ? (b.sort_ts ?? 0) - (a.sort_ts ?? 0)
          : 0);

      searchTotalCount = results.length;
      return results.slice(0, 50) as any;
  }

  async function bootstrapSearchIndex() {
      if (searchIndexBootstrapping) return;
      searchIndexBootstrapping = true;
      try {
          const status = await api.getSearchIndexStatus();
          searchIndexReady = status.ready && status.sessionsCount > 0;
          if (allSessions.length > 0) {
              if (searchIndexReady) {
                  const result = await api.refreshSearchIndex();
                  searchIndexReady = result.indexedSessions > 0;
              } else {
                  const result = await api.rebuildSearchIndex();
                  searchIndexReady = result.indexedSessions > 0;
              }
              if (searchIndexReady) {
                  await refreshSearchIndexStatus();
                  const sessions = await loadSessionInventory(true);
                  applyLoadedSessions(sessions);
              }
          }
      } catch (e) {
          console.error('Search index bootstrap failed:', e);
      } finally {
          searchIndexBootstrapping = false;
      }
  }

  async function performSearch(query: string, source: string, token: number) {
      try {
          const result = await api.searchContent(
              query,
              source,
              getSearchSinceTs(searchTimeRange),
              searchProjectOnly ? currentProject : null,
              50,
              searchSort,
          );
          if (token !== searchRequestToken) return;
          searchTotalCount = result.totalCount;
          if (result.hits.length > 0) {
              searchResults = result.hits.map(hit => ({
                  project: hit.project,
                  source_type: hit.providerId,
                  session_id: hit.sessionId,
                  title: hit.sessionTitle,
                  date: formatTimestamp(hit.messageTimestamp ?? hit.lastActiveAt),
                  snippet: hit.snippet,
                  match_role: hit.messageRole === 'assistant' ? 'assistant' : 'user',
                  match_seq: hit.seq,
                  search_query: query,
                  sort_ts: hit.messageTimestamp ?? hit.lastActiveAt ?? 0,
              }));
              searchIndexReady = true;
              return;
          }
          searchResults = [];
          return;
      } catch (e) {
          console.error('Indexed search failed, falling back to metadata filter:', e);
      }

      if (token !== searchRequestToken) return;
      searchResults = buildMetadataSearchResults(query, source);
  }

  async function handleSearchInput() {
      const query = searchQuery.trim();
      if (!query) {
          searchRequestToken++;
          if (searchTimer) clearTimeout(searchTimer);
          searchResults = [];
          searchTotalCount = 0;
          return;
      }
      if (searchTimer) clearTimeout(searchTimer);
      const token = ++searchRequestToken;
      const source = currentSource;
      searchTimer = setTimeout(() => {
          void performSearch(query, source, token);
      }, 180);
  }

  function openSearch() {
      isIndexModalOpen = false;
      isSearchModalOpen = true;
      setTimeout(() => document.getElementById('searchInput')?.focus(), 50);
  }

  function closeSearch() {
      isSearchModalOpen = false;
      searchQuery = '';
      searchResults = [];
      searchTimeRange = 'all';
      searchSort = 'relevance';
      searchProjectOnly = false;
      searchTotalCount = 0;
  }

  async function openIndexModal() {
      closeSearch();
      indexModalTab = 'overview';
      isIndexModalOpen = true;
      await refreshSearchIndexStatus();
  }

  function closeIndexModal() {
      isIndexModalOpen = false;
  }

  async function setIndexModalTab(tab: IndexModalTab) {
      indexModalTab = tab;
      if (tab === 'sessions') {
          await refreshIndexLibrary(false, true);
      }
  }

  async function setIndexLibraryProviderFilter(providerId: string) {
      indexLibraryProviderFilter = providerId;
      indexLibraryProjectFilter = 'all';
      indexLibraryPage = 1;
      await refreshIndexLibrary(true, true);
  }

  async function setIndexLibraryProjectFilter(projectPath: string) {
      indexLibraryProjectFilter = projectPath;
      await refreshIndexLibrary(true, false);
  }

  async function changeIndexLibraryPage(nextPage: number) {
      const boundedPage = Math.max(1, nextPage);
      if (boundedPage === indexLibraryPage) return;
      indexLibraryPage = boundedPage;
      await refreshIndexLibrary(false, false);
  }

  function setSearchTimeRange(range: SearchTimeRange) {
      searchTimeRange = range;
      if (searchQuery.trim()) void handleSearchInput();
  }

  function setSearchSort(sort: SearchSort) {
      searchSort = sort;
      if (searchQuery.trim()) void handleSearchInput();
  }

  function toggleSearchProjectOnly() {
      searchProjectOnly = !searchProjectOnly;
      if (searchQuery.trim()) void handleSearchInput();
  }

  function handleSearchResultClick(result: SearchResultLocal) {
      closeSearch();
      if (currentProject !== result.project) {
          currentProject = result.project;
      }
      selectConversation(result.session_id, result.source_type, result);
  }

  function handleIndexSessionClick(item: IndexLibraryItem) {
      const session = toSessionMeta(item);
      const targetProject = sessionDir(session);
      currentSource = item.providerId;
      localStorage.setItem('source', item.providerId);
      refreshFromSessions();
      currentProject = targetProject;
      conversations = buildConversations(allSessions, item.providerId, targetProject);
      closeIndexModal();
      void selectConversation(item.sessionId, item.providerId);
  }

  function handleModalBackdropClick(e: MouseEvent) {
      if (e.target === e.currentTarget) {
          closeSearch();
      }
  }

  function handleIndexModalBackdropClick(e: MouseEvent) {
      if (e.target === e.currentTarget) {
          closeIndexModal();
      }
  }

  function handleWindowClick() {
      if (isProjectMenuOpen) {
          isProjectMenuOpen = false;
      }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
      // Disable global hotkeys while user is typing.
      if (document.activeElement?.tagName === 'INPUT' || document.activeElement?.tagName === 'TEXTAREA') {
          if (e.key === 'Escape' && isSearchModalOpen) closeSearch();
          if (e.key === 'Escape' && isIndexModalOpen) closeIndexModal();
          return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
          e.preventDefault();
          openSearch();
      }
      if (e.key === 'Escape') {
          if (isSearchModalOpen) closeSearch();
          else if (isIndexModalOpen) closeIndexModal();
          else if (isProjectMenuOpen) isProjectMenuOpen = false;
          else if (currentView === 'detail') goToConversationList();
      }
      
      if (!isSearchModalOpen && !isIndexModalOpen && currentView === 'list' && projects.length > 0) {
          if (e.key === 'j' || e.key === 'ArrowDown') {
             navigateProject(1);
          } else if (e.key === 'k' || e.key === 'ArrowUp') {
             navigateProject(-1);
          }
      }
  }

  function navigateProject(dir: number) {
      if (!projects.length) return;
      const idx = projects.findIndex(p => p.name === currentProject);
      let newIdx = idx + dir;
      if (newIdx < 0) newIdx = 0;
      if (newIdx >= projects.length) newIdx = projects.length - 1;
      
      if (newIdx !== idx) {
          const proj = projects[newIdx];
          selectProject(proj.name);
           const el = document.querySelector(`[data-project="${proj.name}"]`);
           el?.scrollIntoView({ block: 'nearest' });
      }
  }

  function parseIndexDate(iso?: string): Date | null {
      if (!iso) return null;
      const normalized = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(iso)
          ? iso.replace(' ', 'T') + 'Z'
          : iso;
      const date = new Date(normalized);
      return Number.isNaN(date.getTime()) ? null : date;
  }

  function formatRelativeTime(iso?: string): string {
      if (!iso) return t('common.never');

      const date = parseIndexDate(iso);
      if (!date) return iso;
      const diff = Date.now() - date.getTime();
      if (Number.isNaN(diff)) return iso;
      if (diff < 0) return t('common.just_now');

      const minutes = Math.floor(diff / 60000);
      if (minutes < 1) return t('common.just_now');
      const formatter = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
      if (minutes < 60) return formatter.format(-minutes, 'minute');
      const hours = Math.floor(minutes / 60);
      if (hours < 24) return formatter.format(-hours, 'hour');
      return formatter.format(-Math.floor(hours / 24), 'day');
  }

  function formatIndexDateTime(iso?: string): string {
      const date = parseIndexDate(iso);
      if (!date) return iso ?? t('common.na');
      return new Intl.DateTimeFormat(locale, {
          year: 'numeric',
          month: 'numeric',
          day: 'numeric',
          hour: 'numeric',
          minute: '2-digit',
          second: '2-digit',
      }).format(date);
  }

  function providerDisplayName(providerId: string): string {
      return ({
          claude: 'Claude CLI',
          codex: 'Codex CLI',
          gemini: 'Gemini CLI',
          openclaw: 'OpenClaw',
          opencode: 'OpenCode',
      } as Record<string, string>)[providerId] ?? providerId;
  }

  function formatConversationSessionId(sessionId: string): string {
      return `(#${sessionId})`;
  }

  function getProjectListLabel(projectPath: string): string {
      if (!compactProjectPaths) return projectPath;
      return baseName(projectPath) || projectPath;
  }

  function formatBytes(bytes?: number): string {
      if (!bytes || bytes <= 0) return '0 B';
      const units = ['B', 'KB', 'MB', 'GB'];
      let value = bytes;
      let unitIndex = 0;
      while (value >= 1024 && unitIndex < units.length - 1) {
          value /= 1024;
          unitIndex += 1;
      }
      return `${value >= 10 || unitIndex === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
  }

  function syncPhaseLabel(phase?: string): string {
      if (phase === 'refreshing') return t('index.sync_phase.refreshing');
      if (phase === 'scanning') return t('index.sync_phase.scanning');
      if (phase === 'syncing') return t('index.sync_phase.syncing');
      if (phase === 'done') return t('index.sync_phase.done');
      return t('index.sync_phase.idle');
  }

  function syncPhaseMessage(phase?: string): UiMessage | null {
      if (phase === 'refreshing') return keyMessage('index.sync_message.refreshing');
      if (phase === 'scanning') return keyMessage('index.sync_message.scanning');
      if (phase === 'syncing') return keyMessage('index.sync_message.syncing');
      return null;
  }

  const sourceLabel = $derived(({
      'claude': 'Claude CLI',
      'codex': 'Codex CLI',
      'gemini': 'Gemini CLI',
      'openclaw': 'OpenClaw',
      'opencode': 'OpenCode'
  } as Record<string, string>)[currentSource] || t('common.history'));
  const selectedSession = $derived(
      currentConversation
          ? getSessionById(currentConversation.session_id, currentConversation.source_type)
          : null,
  );
  const indexStatusText = $derived(
      searchIndexStatus?.ready
          ? t('index.status.indexed', { relative: formatRelativeTime(searchIndexStatus.lastIndexedAt) })
          : t('index.status.unavailable'),
  );
  const indexSessionPreviewCount = $derived(indexLibraryItems.length);
  const indexLibraryPageCount = $derived(
      Math.max(1, Math.ceil(indexLibraryTotalCount / INDEX_LIBRARY_PAGE_SIZE)),
  );
  const indexLibraryRangeText = $derived(
      indexLibraryTotalCount === 0
          ? t('index.range_empty')
          : t('index.range_value', {
              start: (indexLibraryPage - 1) * INDEX_LIBRARY_PAGE_SIZE + 1,
              end: Math.min(indexLibraryPage * INDEX_LIBRARY_PAGE_SIZE, indexLibraryTotalCount),
              total: indexLibraryTotalCount,
            }),
  );
  const indexPaginationText = $derived(
      t('index.page_info', {
          page: indexLibraryPage,
          pages: indexLibraryPageCount,
          loaded: indexSessionPreviewCount,
      }),
  );
  const indexSyncText = $derived(
      searchIndexSyncInfo
          ? `${syncPhaseLabel(searchIndexSyncInfo.phase)}${
              searchIndexSyncInfo.total > 0 ? ` ${searchIndexSyncInfo.count}/${searchIndexSyncInfo.total}` : ''
            }`
          : t('index.sync_phase.idle'),
  );

</script>

{#if isWebMode && !authInitialized}
<div class="auth-shell">
  <div class="auth-card auth-card-loading">
    <div class="auth-badge">ACLIV Web</div>
    <h1>{t('auth.checking_status_title')}</h1>
    <p>{t('auth.checking_status_body')}</p>
  </div>
</div>
{:else if isWebMode && !isAuthenticated}
<div class="auth-shell">
  <button class="action-btn auth-theme-toggle" onclick={toggleTheme} type="button" title={t('common.theme.toggle')}>
      {#if theme === 'light'}
        <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M9.598 1.591a.75.75 0 01.785-.175 7 7 0 11-8.967 8.967.75.75 0 01.961-.96 5.5 5.5 0 007.046-7.046.75.75 0 01.175-.786zm1.616 1.945a7 7 0 01-7.678 7.678 5.5 5.5 0 107.678-7.678z"></path></svg>
      {:else}
        <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 12a4 4 0 100-8 4 4 0 000 8zM8 0a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 0zm0 13a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 13zM2.343 2.343a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L2.343 3.05a.5.5 0 010-.707zm11.314 8.486a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM12.914 2.343a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM3.05 12.207a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L3.05 12.914a.5.5 0 010-.707zM13 8a.5.5 0 01.5.5h2a.5.5 0 010-1h-2A.5.5 0 0113 8zM0 8a.5.5 0 01.5-.5h2a.5.5 0 010 1h-2A.5.5 0 010 8z"></path></svg>
      {/if}
  </button>
  <div class="auth-card">
    <div class="auth-badge">ACLIV Web</div>
    <h1>{t('auth.login_title')}</h1>
    <form class="auth-form" onsubmit={(event) => { event.preventDefault(); void handleLoginSubmit(); }}>
      <label class="auth-field">
        <span>{t('auth.username')}</span>
        <input bind:value={loginUsername} type="text" autocomplete="username" placeholder="admin" />
      </label>
      <label class="auth-field">
        <span>{t('auth.password')}</span>
        <input bind:value={loginPassword} type="password" autocomplete="current-password" placeholder={t('auth.password_placeholder')} />
      </label>
      {#if loginError}
        <div class="auth-error">{resolveMessage(loginError)}</div>
      {/if}
      <button class="auth-submit" type="submit" disabled={isLoggingIn}>
        {isLoggingIn ? t('auth.submitting') : t('auth.submit')}
      </button>
    </form>
  </div>
</div>
{:else}
<div class="app-container">
  <aside class="sidebar">
    <div class="sidebar-header">
      <div class="source-selector">
        <button class="source-toggle" class:active={isSourceDropdownOpen} onclick={() => isSourceDropdownOpen = !isSourceDropdownOpen} type="button">
            <span class="source-title">{sourceLabel}</span>
            <span class="dropdown-arrow">{@html ICONS.dropdown_arrow}</span>
        </button>
        
        <div class="source-dropdown" class:show={isSourceDropdownOpen}>
            {#each sources as src}
                <button class="source-item" class:selected={currentSource === src} onclick={() => selectSource(src)} type="button">
                    {src === 'claude' ? 'Claude CLI' : src === 'codex' ? 'Codex CLI' : src === 'gemini' ? 'Gemini CLI' : src === 'openclaw' ? 'OpenClaw' : 'OpenCode'}
                </button>
            {/each}
        </div>
      </div>
      
      <div class="stats" id="stats">
        <span>{@html getIcon('project', 14)} {stats.projects_count}</span>
        <span>{@html getIcon('conversation', 14)} {stats.conversations_count}</span>
        <span>{@html getIcon('message', 14)} {stats.messages_count}</span>
      </div>
    </div>

    <div class="sidebar-content" id="projectsList">
        <div class="projects-list">
            {#each projects as project}
                <button class="project-item" 
                     class:active={currentProject === project.name}
                     data-project={project.name}
                     title={compactProjectPaths ? project.name : undefined}
                     onclick={() => selectProject(project.name)}
                     type="button">
                    <span class="project-item-main">
                        <span class="project-item-icon" aria-hidden="true">
                            {@html getIcon('folderOutline', 15)}
                        </span>
                        <span class="project-name">{getProjectListLabel(project.name)}</span>
                    </span>
                    <span class="project-count">{project.conversation_count}</span>
                </button>
            {/each}
        </div>
    </div>
  </aside>

  <main class="main-content">
     <div class="view" class:active={currentView === 'list'} id="listView">
     <div class="view-header">
             <h2>{currentProject || t('detail.select_project')}</h2>
             {#if projects.length > 0 && currentProject}
                <span class="view-info">{t('common.count.conversations', { count: conversations.length })}</span>
             {/if}
             <div class="view-header-actions">
                 <button class="action-btn" id="openSearchBtn" onclick={openSearch} type="button">
                    {@html getIcon('search', 16)}
                 </button>
                 <button class="action-btn index-toggle-btn" class:index-ready={searchIndexStatus?.ready} onclick={() => void openIndexModal()} type="button" title={t('index.title')}>
                    {@html getIcon('database', 16)}
                 </button>
                 <button class="action-btn theme-toggle" id="themeToggle" onclick={toggleTheme} type="button" title={t('common.theme.toggle')}>
                     {#if theme === 'light'}
                       <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M9.598 1.591a.75.75 0 01.785-.175 7 7 0 11-8.967 8.967.75.75 0 01.961-.96 5.5 5.5 0 007.046-7.046.75.75 0 01.175-.786zm1.616 1.945a7 7 0 01-7.678 7.678 5.5 5.5 0 107.678-7.678z"></path></svg>
                     {:else}
                       <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 12a4 4 0 100-8 4 4 0 000 8zM8 0a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 0zm0 13a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 13zM2.343 2.343a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L2.343 3.05a.5.5 0 010-.707zm11.314 8.486a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM12.914 2.343a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM3.05 12.207a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L3.05 12.914a.5.5 0 010-.707zM13 8a.5.5 0 01.5.5h2a.5.5 0 010-1h-2A.5.5 0 0113 8zM0 8a.5.5 0 01.5-.5h2a.5.5 0 010 1h-2A.5.5 0 010 8z"></path></svg>
                     {/if}
                 </button>
             </div>
         </div>
         <div class="conversations-list" id="conversationsList">
            {#if conversations.length === 0}
               <div class="empty-state">
                   {@html ICONS.empty_box}
                   <h3>{t('detail.no_conversations')}</h3>
               </div>
            {:else}
                {#each conversations as conv}
                    <button class="conversation-item" onclick={() => selectConversation(conv.session_id, conv.source_type)} type="button">
                        <div class="conversation-title">
                            <span>{conv.title}</span>
                            {#if showSessionIds}
                                <span class="conversation-session-id">{formatConversationSessionId(conv.session_id)}</span>
                            {/if}
                        </div>
                        <div class="conversation-meta">
                            <span class="meta-item">{@html getIcon('conversation', 12)} {t('common.count.messages', { count: conv.message_count })}</span>
                            <span class="meta-item">{@html getIcon('clock', 12)} {conv.date}</span>
                        </div>
                    </button>
               {/each}
            {/if}
         </div>
     </div>

     <div class="view" class:active={currentView === 'detail'} id="detailView">
     <div class="view-header">
             <button class="btn-secondary" id="backBtn" onclick={() => goToConversationList()} type="button">
                 {@html ICONS.back} {t('actions.back')}
             </button>
              <h2>{selectedSession ? sessionTitle(selectedSession) : currentConversation?.title || t('common.conversation')}</h2>
             {#if currentConversation}
                 <div class="view-header-actions">
                     <button class="action-btn" onclick={openSearch} type="button">
                        {@html getIcon('search', 16)}
                     </button>
                     <button class="action-btn index-toggle-btn" class:index-ready={searchIndexStatus?.ready} onclick={() => void openIndexModal()} type="button" title={t('index.title')}>
                        {@html getIcon('database', 16)}
                     </button>
                     <button class="action-btn theme-toggle" onclick={toggleTheme} type="button" title={t('common.theme.toggle')}>
                         {#if theme === 'light'}
                           <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M9.598 1.591a.75.75 0 01.785-.175 7 7 0 11-8.967 8.967.75.75 0 01.961-.96 5.5 5.5 0 007.046-7.046.75.75 0 01.175-.786zm1.616 1.945a7 7 0 01-7.678 7.678 5.5 5.5 0 107.678-7.678z"></path></svg>
                         {:else}
                           <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 12a4 4 0 100-8 4 4 0 000 8zM8 0a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 0zm0 13a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 13zM2.343 2.343a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L2.343 3.05a.5.5 0 010-.707zm11.314 8.486a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM12.914 2.343a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM3.05 12.207a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L3.05 12.914a.5.5 0 010-.707zM13 8a.5.5 0 01.5.5h2a.5.5 0 010-1h-2A.5.5 0 0113 8zM0 8a.5.5 0 01.5-.5h2a.5.5 0 010 1h-2A.5.5 0 010 8z"></path></svg>
                         {/if}
                    </button>
                    <button class="btn-danger" onclick={openDeleteDialog} type="button" disabled={isDeleting}>
                        <span class="icon-inline" aria-hidden="true">
                            {@html getIcon('trash', 15)}
                        </span>
                        <span>{isDeleting ? t('actions.deleting') : t('actions.delete')}</span>
                    </button>
                </div>
            {/if}
        </div>
        <div
            class="conversation-detail"
            id="conversationDetail"
            bind:this={conversationDetailElement}
            onscroll={handleConversationDetailScroll}
        >
            {#if currentConversation}
                <div class="conversation-header" bind:this={conversationHeaderElement}>
                    <h3>{selectedSession ? sessionTitle(selectedSession) : currentConversation.title}</h3>
                    <div class="conversation-info">
                        {#if showSessionIds}
                            <span>{@html getIcon('message', 12)} {t('common.id')}: {currentConversation.session_id}</span>
                        {/if}
                        <span>{@html getIcon('clock', 12)} {currentConversation.timestamp || t('common.na')}</span>
                        {#if selectedSession?.projectDir}
                            <div class="menu-anchor project-menu-anchor">
                                <button
                                    class="conversation-info-btn"
                                    type="button"
                                    aria-expanded={isProjectMenuOpen}
                                    onclick={toggleProjectMenu}
                                >
                                    <span class="icon-inline folder-info-icon" aria-hidden="true">
                                        {@html getIcon('folderOpen', 13)}
                                    </span>
                                    <span>{baseName(selectedSession.projectDir)}</span>
                                </button>
                                <div class="hover-menu" class:show-menu={isProjectMenuOpen}>
                                    <button type="button" onclick={handleProjectPathCopy}>
                                        {t('actions.copy_path')}
                                    </button>
                                    {#if !isWebMode}
                                        <button type="button" onclick={openProjectInExplorer}>
                                            {t('actions.open_in_explorer')}
                                        </button>
                                    {/if}
                                </div>
                            </div>
                        {/if}
                    </div>
                    {#if selectedSession?.resumeCommand}
                        <div class="detail-card">
                            <div class="detail-card-header">
                                <span class="detail-card-label">{t('detail.resume_command')}</span>
                                <div class="detail-card-actions">
                                    <button
                                        class="inline-icon-btn"
                                        onclick={() => copyText(selectedSession.resumeCommand!, keyMessage('toast.resume_command_copied'))}
                                        type="button"
                                        title={t('detail.copy_resume_command')}
                                    >
                                        {@html getIcon('copy', 14)}
                                    </button>
                                    {#if !isWebMode}
                                        <div class="menu-anchor">
                                            <button
                                                class="inline-icon-btn"
                                                type="button"
                                                title={t('detail.open_terminal')}
                                            >
                                                {@html getIcon('terminal', 14)}
                                            </button>
                                            <div class="hover-menu">
                                                <button type="button" onclick={() => openResumeTerminal('cmd')}>
                                                    {t('detail.open_in_cmd')}
                                                </button>
                                                <button type="button" onclick={() => openResumeTerminal('powershell')}>
                                                    {t('detail.open_in_powershell')}
                                                </button>
                                            </div>
                                        </div>
                                    {/if}
                                </div>
                            </div>
                            <div class="detail-card-value detail-card-code">{selectedSession.resumeCommand}</div>
                        </div>
                    {/if}
                </div>
                <div class="messages-container" bind:this={messagesContainerElement}>
                    {#each getVisibleConversationBlocks(currentConversation.blocks) as block, i}
                        {#if isToolGroupBlock(block)}
                            <details class={`message message-collapsible ${getMessageBlockClass(block)}`}>
                                <summary class="tool-group-summary">
                                    <div class="tool-group-summary-main">
                                        <div class="tool-group-summary-copy">
                                            <span class="tool-group-title">{getToolGroupSummary(block)}</span>
                                            {#if block.timestamp}
                                                <span class="message-ts">{block.timestamp}</span>
                                            {/if}
                                        </div>
                                    </div>
                                </summary>
                                <div class="tool-group-body">
                                    {#each getToolGroupChildren(block) as toolBlock}
                                        <details class={`message message-collapsible message-in-tool-group ${getMessageBlockClass(toolBlock)}`}>
                                            <summary class="message-header">
                                                <div class="message-header-main">
                                                    <span class="message-role">{getMessageBlockLabel(toolBlock)}</span>
                                                    <span class="message-number">{getToolDetailNumber(toolBlock)}</span>
                                                    {#if toolBlock.name}
                                                        <span class="tool-call-name">{toolBlock.name}</span>
                                                    {/if}
                                                </div>
                                                <div class="message-header-side">
                                                    {#if toolBlock.timestamp}
                                                        <span class="message-ts">{toolBlock.timestamp}</span>
                                                    {/if}
                                                </div>
                                            </summary>
                                            <div class="message-collapsible-body">
                                                <div class="message-collapsible-meta-row">
                                                    {#if toolBlock.callId}
                                                        <div class="tool-call-meta">{t('detail.call_id')}: {toolBlock.callId}</div>
                                                    {/if}
                                                    <button
                                                        class="inline-icon-btn"
                                                        onclick={() => copyText(toolBlock.content, keyMessage('toast.message_copied'))}
                                                        type="button"
                                                        title={t('detail.copy_message')}
                                                    >
                                                        {@html getIcon('copy', 14)}
                                                    </button>
                                                </div>
                                                {#if isRawTextBlock(toolBlock)}
                                                    <pre class="message-content message-raw-content">{toolBlock.content}</pre>
                                                {:else if MarkdownComponent}
                                                    <MarkdownComponent content={toolBlock.content} />
                                                {:else}
                                                    <pre class="message-content message-raw-content">{toolBlock.content}</pre>
                                                {/if}
                                            </div>
                                        </details>
                                    {/each}
                                </div>
                            </details>
                        {:else if isCollapsibleBlock(block)}
                            <details class={`message message-collapsible ${getMessageBlockClass(block)}`}>
                                <summary class="message-header">
                                    <div class="message-header-main">
                                        <span class="message-role">{getMessageBlockLabel(block)}</span>
                                        <span class="message-number">#{i + 1}</span>
                                        {#if block.name}
                                            <span class="tool-call-name">{block.name}</span>
                                        {/if}
                                    </div>
                                    <div class="message-header-side">
                                        {#if block.timestamp}
                                            <span class="message-ts">{block.timestamp}</span>
                                        {/if}
                                    </div>
                                </summary>
                                <div class="message-collapsible-body">
                                    <div class="message-collapsible-meta-row">
                                        {#if block.callId}
                                            <div class="tool-call-meta">{t('detail.call_id')}: {block.callId}</div>
                                        {/if}
                                        <button
                                            class="inline-icon-btn"
                                            onclick={() => copyText(block.content, keyMessage('toast.message_copied'))}
                                            type="button"
                                            title={t('detail.copy_message')}
                                        >
                                            {@html getIcon('copy', 14)}
                                        </button>
                                    </div>
                                    {#if isRawTextBlock(block)}
                                        <pre class="message-content message-raw-content">{block.content}</pre>
                                    {:else if MarkdownComponent}
                                        <MarkdownComponent content={block.content} />
                                    {:else}
                                        <pre class="message-content message-raw-content">{block.content}</pre>
                                    {/if}
                                </div>
                            </details>
                        {:else}
                            <div
                                class={`message ${getMessageBlockClass(block)}`}
                                class:search-hit={isBlockSearchMatch(block)}
                                data-progress-anchor={isConversationProgressBlock(block) ? 'true' : undefined}
                                data-progress-key={isConversationProgressBlock(block) ? getConversationProgressKey(block) : undefined}
                                data-progress-label={isConversationProgressBlock(block) ? getConversationProgressLabel(block) : undefined}
                                data-progress-preview={isConversationProgressBlock(block) ? getConversationProgressPreview(block) : undefined}
                            >
                                <div class="message-header">
                                    <div class="message-header-main">
                                        <span class="message-role">{getMessageBlockLabel(block)}</span>
                                        <span class="message-number">#{i + 1}</span>
                                        {#if block.name}
                                            <span class="tool-call-name">{block.name}</span>
                                        {/if}
                                    </div>
                                    <div class="message-header-side">
                                        {#if block.timestamp}
                                            <span class="message-ts">{block.timestamp}</span>
                                        {/if}
                                        <button
                                            class="inline-icon-btn message-copy-btn"
                                            onclick={() => copyText(block.content, keyMessage('toast.message_copied'))}
                                            type="button"
                                            title={t('detail.copy_message')}
                                        >
                                            {@html getIcon('copy', 14)}
                                        </button>
                                    </div>
                                </div>
                                {#if MarkdownComponent}
                                    <MarkdownComponent content={block.content} />
                                {:else}
                                    <pre class="message-content message-raw-content">{block.content}</pre>
                                {/if}
                            </div>
                        {/if}
                    {/each}
                </div>
            {/if}
        </div>
     </div>

      {#if currentView === 'detail' && currentConversation}
        {#if conversationProgressAnchors.length > 0}
            <div
                class="detail-progress-nav"
                class:expanded={isConversationProgressExpanded}
                style={`height: ${getConversationProgressNavHeight(conversationProgressAnchors.length)}`}
                role="group"
                aria-label={t('detail.conversation_progress')}
                onmouseenter={openConversationProgressDirectory}
                onmouseleave={closeConversationProgressDirectory}
            >
                {#each conversationProgressAnchors as anchor, index}
                    <button
                        class="detail-progress-anchor detail-progress-mark"
                        class:active={anchor.key === activeConversationProgressKey}
                        style={`top: ${getConversationProgressAnchorTop(index)}`}
                        type="button"
                        aria-label={anchor.label}
                        onclick={() => jumpToConversationProgressAnchor(anchor)}
                    ></button>
                {/each}
                <div class="detail-progress-directory">
                    <div class="detail-progress-directory-title">{t('detail.user_directory')}</div>
                    <div class="detail-progress-directory-list">
                        {#each conversationProgressAnchors as anchor}
                            <button
                                class="detail-progress-item"
                                class:active={anchor.key === activeConversationProgressKey}
                                type="button"
                                title={anchor.label}
                                onclick={() => jumpToConversationProgressAnchor(anchor)}
                            >
                                <span class="detail-progress-item-line" aria-hidden="true"></span>
                                <span class="detail-progress-item-label">{anchor.label}</span>
                            </button>
                        {/each}
                    </div>
                </div>
            </div>
        {/if}
        <button
            class="detail-refresh-fab"
            type="button"
            onclick={() => void refreshCurrentConversation()}
            disabled={isConversationRefreshing}
            title={t('detail.refresh_current')}
        >
            <span class="icon-inline" class:fab-icon-spinning={isConversationRefreshing} aria-hidden="true">
                {@html getIcon('refresh', 16)}
            </span>
        </button>
     {/if}

  <div class="refresh-toast" class:show={showToast}>
      <div class="refresh-content" class:syncing={toastType === 'syncing'} class:success={toastType === 'success'} class:error={toastType === 'error'}>
          {#if toastType === 'syncing'}
              <svg class="spinner-small" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
              </svg>
              <span>{resolveMessage(toastMessage)}</span>
          {:else}
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M20 6 9 17 4 12"></path>
              </svg>
              <span>{resolveMessage(toastMessage)}</span>
          {/if}
      </div>
  </div>
</main>

  <div class="search-modal" id="searchModal" 
       class:active={isSearchModalOpen} 
       role="button" 
       tabindex="0"
       onclick={handleModalBackdropClick}
       onkeydown={(e) => e.key === 'Escape' && closeSearch()}>
      <div class="search-container">
           <!-- svelte-ignore a11y_no_noninteractive_element_interactions a11y_no_static_element_interactions -->
          <div class="search-input-wrapper" role="button" tabindex="0" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
              {@html getIcon('search', 16)}
              <input type="text" id="searchInput" placeholder={t('search.placeholder')}
                     bind:value={searchQuery} 
                     oninput={handleSearchInput} />
              <button class="btn-close-search" onclick={closeSearch} type="button">ESC</button>
          </div>
          <div class="search-filter-bar">
              <div class="search-filter-group">
                  <button class:active={searchTimeRange === 'all'} class="filter-chip" type="button" onclick={() => setSearchTimeRange('all')}>{t('search.all')}</button>
                  <button class:active={searchTimeRange === '7d'} class="filter-chip" type="button" onclick={() => setSearchTimeRange('7d')}>7d</button>
                  <button class:active={searchTimeRange === '30d'} class="filter-chip" type="button" onclick={() => setSearchTimeRange('30d')}>30d</button>
                  <button class:active={searchTimeRange === '90d'} class="filter-chip" type="button" onclick={() => setSearchTimeRange('90d')}>90d</button>
              </div>
              <div class="search-filter-group search-filter-group-right">
                  <span class="search-results-count">
                      {#if searchQuery.trim()}
                          {t('common.count.results', { count: searchTotalCount })}
                      {:else}
                          {t('search.type_to_search')}
                      {/if}
                  </span>
                  <button class:active={searchSort === 'relevance'} class="filter-chip" type="button" onclick={() => setSearchSort('relevance')}>
                      {t('search.relevance')}
                  </button>
                  <button class:active={searchSort === 'recent'} class="filter-chip" type="button" onclick={() => setSearchSort('recent')}>
                      {t('search.recent')}
                  </button>
                  {#if currentProject}
                      <button class:active={searchProjectOnly} class="filter-chip" type="button" onclick={toggleSearchProjectOnly}>
                          {searchProjectOnly ? t('search.current_project') : t('search.all_projects')}
                      </button>
                  {/if}
              </div>
          </div>
          <div class="search-modal-results" id="searchModalResults">
              {#if searchQuery.trim() && searchResults.length === 0}
                  <div class="index-library-empty search-empty-state">{t('search.no_results')}</div>
              {:else}
                  {#each searchResults as result}
                      <button class="conversation-item" onclick={() => handleSearchResultClick(result)} type="button">
                          <div class="conversation-title">
                              <span>{result.title}</span>
                              {#if showSessionIds}
                                  <span class="conversation-session-id">{formatConversationSessionId(result.session_id)}</span>
                              {/if}
                          </div>
                          {#if result.snippet}
                              <div class="conversation-snippet search-snippet">{@html result.snippet}</div>
                          {/if}
                           <div class="conversation-meta">
                                <span class="meta-item">{@html getIcon('project', 12)} {result.project}</span>
                                <span class="meta-item">{@html getIcon('clock', 12)} {result.date}</span>
                           </div>
                      </button>
                  {/each}
              {/if}
          </div>
      </div>
  </div>

  <div
      class="index-modal"
      class:active={isIndexModalOpen}
      role="button"
      tabindex="0"
      onclick={handleIndexModalBackdropClick}
      onkeydown={(e) => e.key === 'Escape' && closeIndexModal()}
  >
      <div class="index-modal-card">
          <div class="index-modal-header">
              <div class="index-modal-title">
                  <div class="index-modal-title-row">
                      <span class:status-ok={searchIndexStatus?.ready} class="index-status-dot"></span>
                      <h3>{t('index.title')}</h3>
                  </div>
                  <p>{indexStatusText}</p>
              </div>
              <button class="btn-close-search" onclick={closeIndexModal} type="button">ESC</button>
          </div>

          <div class="index-tab-bar">
              <button
                  class="index-tab-btn"
                  class:active={indexModalTab === 'overview'}
                  type="button"
                  onclick={() => void setIndexModalTab('overview')}
              >
                  {t('index.overview')}
              </button>
              <button
                  class="index-tab-btn"
                  class:active={indexModalTab === 'sessions'}
                  type="button"
                  onclick={() => void setIndexModalTab('sessions')}
              >
                  {t('index.indexed_sessions')}
              </button>
          </div>

          {#if indexModalTab === 'overview'}
              <div class="index-modal-summary">
                  <div class="index-summary-card">
                      <span class="index-summary-label">{t('index.database')}</span>
                      <span class="index-summary-value">{searchIndexStatus?.dbPath || t('common.unavailable')}</span>
                  </div>
                  <div class="index-summary-grid">
                      <div class="index-summary-card">
                          <span class="index-summary-label">{t('index.sync_status')}</span>
                          <strong>{indexSyncText}</strong>
                          {#if searchIndexSyncInfo?.message}
                              <span class="index-summary-subtle">{resolveMessage(searchIndexSyncInfo.message)}</span>
                          {/if}
                      </div>
                      <div class="index-summary-card">
                          <span class="index-summary-label">{t('index.projects')}</span>
                          <strong>{searchIndexStatus?.projectsCount ?? 0}</strong>
                      </div>
                      <div class="index-summary-card">
                          <span class="index-summary-label">{t('index.sessions')}</span>
                          <strong>{searchIndexStatus?.sessionsCount ?? 0}</strong>
                      </div>
                      <div class="index-summary-card">
                          <span class="index-summary-label">{t('index.messages')}</span>
                          <strong>{searchIndexStatus?.messagesCount ?? 0}</strong>
                      </div>
                      <div class="index-summary-card">
                          <span class="index-summary-label">{t('index.database_size')}</span>
                          <strong>{formatBytes(searchIndexStatus?.dbSizeBytes)}</strong>
                      </div>
                      <div class="index-summary-card">
                          <span class="index-summary-label">{t('index.last_indexed')}</span>
                          <div class="index-summary-hover">
                              <span class="index-summary-hover-chip">{t('index.status')}</span>
                              <div class="index-summary-tooltip">
                                  <div class="index-summary-tooltip-row">
                                      <span>{t('index.last_success')}</span>
                                      <strong>{formatIndexDateTime(searchIndexStatus?.lastSuccessfulSyncAt)}</strong>
                                  </div>
                                  <div class="index-summary-tooltip-row">
                                      <span>{t('index.last_error')}</span>
                                      <strong>{formatIndexDateTime(searchIndexStatus?.lastErrorAt)}</strong>
                                  </div>
                              </div>
                          </div>
                          <strong>{formatIndexDateTime(searchIndexStatus?.lastIndexedAt)}</strong>
                      </div>
                      <div class="index-summary-card">
                          <span class="index-summary-label">{t('index.errors')}</span>
                          <strong>{searchIndexStatus?.errorCount ?? 0}</strong>
                      </div>
                  </div>
                  {#if searchIndexStatus?.sources?.length}
                      <div class="index-source-grid">
                          {#each searchIndexStatus.sources as source}
                              <div class="index-source-card">
                                  <div class="index-source-head">
                                      <strong>{providerDisplayName(source.providerId)}</strong>
                                      <span>{t('common.count.sessions', { count: source.sessionsCount })}</span>
                                  </div>
                                  <div class="index-source-metrics">
                                      <span>{t('common.count.projects', { count: source.projectsCount })}</span>
                                      <span>{t('common.count.messages', { count: source.messagesCount })}</span>
                                  </div>
                              </div>
                          {/each}
                      </div>
                  {/if}
                  <div class="index-status-actions">
                      <button class="index-action-btn" type="button" onclick={() => runSearchIndexAction('refresh')} disabled={isIndexActionRunning}>
                          {t('actions.sync')}
                      </button>
                      <button class="index-action-btn" type="button" onclick={() => runSearchIndexAction('rebuild')} disabled={isIndexActionRunning}>
                          {t('actions.rebuild')}
                      </button>
                      <div class="index-locale-switch" role="group" aria-label={t('index.language')}>
                          <span class="index-locale-label">{t('index.language')}</span>
                          <div class="index-locale-segments">
                              <button
                                  class="index-locale-btn"
                                  class:active={locale === 'zh'}
                                  type="button"
                                  onclick={() => updateLocale('zh')}
                              >
                                  {t('index.locale.zh')}
                              </button>
                              <button
                                  class="index-locale-btn"
                                  class:active={locale === 'en'}
                                  type="button"
                                  onclick={() => updateLocale('en')}
                              >
                                  {t('index.locale.en')}
                              </button>
                          </div>
                      </div>
                      <button
                          class="index-switch-btn"
                          class:active={compactProjectPaths}
                          type="button"
                          role="switch"
                          aria-checked={compactProjectPaths}
                          onclick={toggleCompactProjectPaths}
                      >
                          <span class="index-switch-label">{t('index.compact_paths')}</span>
                          <span class="index-switch-track" aria-hidden="true">
                              <span class="index-switch-thumb"></span>
                          </span>
                      </button>
                      <button
                          class="index-switch-btn"
                          class:active={hideFunctionCalls}
                          type="button"
                          role="switch"
                          aria-checked={hideFunctionCalls}
                          onclick={toggleHideFunctionCalls}
                      >
                          <span class="index-switch-label">{t('index.hide_tool_calls')}</span>
                          <span class="index-switch-track" aria-hidden="true">
                              <span class="index-switch-thumb"></span>
                          </span>
                      </button>
                      <button
                          class="index-switch-btn"
                          class:active={showSessionIds}
                          type="button"
                          role="switch"
                          aria-checked={showSessionIds}
                          onclick={toggleShowSessionIds}
                      >
                          <span class="index-switch-label">{t('index.show_session_ids')}</span>
                          <span class="index-switch-track" aria-hidden="true">
                              <span class="index-switch-thumb"></span>
                          </span>
                      </button>
                  </div>
              </div>
          {:else}
              <div class="index-library-section">
                  <div class="index-library-header">
                      <div>
                          <h4>{t('index.indexed_sessions')}</h4>
                          <p>{t('index.open_hint')}</p>
                      </div>
                      <span class="view-info">{indexLibraryRangeText}</span>
                  </div>

                  <div class="index-library-filters">
                      <label class="index-filter-field">
                          <span>{t('common.provider')}</span>
                          <select
                              class="index-filter-select"
                              value={indexLibraryProviderFilter}
                              onchange={(event) => void setIndexLibraryProviderFilter((event.currentTarget as HTMLSelectElement).value)}
                          >
                              <option value="all">{t('index.all_providers')}</option>
                              {#each sources as source}
                                  <option value={source}>{providerDisplayName(source)}</option>
                              {/each}
                          </select>
                      </label>
                      <label class="index-filter-field">
                          <span>{t('common.project')}</span>
                          <select
                              class="index-filter-select"
                              value={indexLibraryProjectFilter}
                              onchange={(event) => void setIndexLibraryProjectFilter((event.currentTarget as HTMLSelectElement).value)}
                          >
                              <option value="all">{t('index.all_projects')}</option>
                              {#each indexProjectOptions as project}
                                  <option value={project.project}>
                                      {project.projectName || baseName(project.project) || project.project} ({project.sessionsCount})
                                  </option>
                              {/each}
                          </select>
                      </label>
                  </div>

                  <div class="index-library-list">
                      {#if isIndexLibraryLoading}
                          <div class="index-library-empty">{t('index.loading_sessions')}</div>
                      {:else if indexLibraryItems.length === 0}
                          <div class="index-library-empty">{t('index.no_sessions')}</div>
                      {:else}
                          {#each indexLibraryItems as item}
                              <button class="index-library-item" type="button" onclick={() => handleIndexSessionClick(item)}>
                                  <div class="index-library-item-top">
                                      <strong>{indexedSessionTitle(item)}</strong>
                                      <span class="index-provider-pill">{providerDisplayName(item.providerId)}</span>
                                  </div>
                                  <div class="index-library-meta">
                                      <span class="meta-item">{@html getIcon('project', 12)} {item.projectName || baseName(item.project) || item.project}</span>
                                      <span class="meta-item">{@html getIcon('message', 12)} {t('common.count.messages', { count: item.messageCount })}</span>
                                      <span class="meta-item">{@html getIcon('clock', 12)} {formatTimestamp(item.lastActiveAt ?? item.createdAt)}</span>
                                  </div>
                                  {#if item.model || item.cwd}
                                      <div class="index-library-extra">
                                          {#if item.model}
                                              <span>{t('common.model')}: {item.model}</span>
                                          {/if}
                                          {#if item.cwd}
                                              <span>{t('common.cwd')}: {item.cwd}</span>
                                          {/if}
                                      </div>
                                  {/if}
                              </button>
                          {/each}
                      {/if}
                  </div>

                  <div class="index-library-pagination">
                      <span class="index-pagination-info">{indexPaginationText}</span>
                      <div class="index-pagination-actions">
                          <button
                              class="index-page-btn"
                              type="button"
                              onclick={() => void changeIndexLibraryPage(indexLibraryPage - 1)}
                              disabled={isIndexLibraryLoading || indexLibraryPage <= 1}
                          >
                              {t('actions.previous')}
                          </button>
                          <button
                              class="index-page-btn"
                              type="button"
                              onclick={() => void changeIndexLibraryPage(indexLibraryPage + 1)}
                              disabled={isIndexLibraryLoading || indexLibraryPage >= indexLibraryPageCount}
                          >
                              {t('actions.next')}
                          </button>
                      </div>
                  </div>
              </div>
          {/if}
      </div>
  </div>

  <div
      class="confirm-modal"
      class:active={!!deleteTarget}
      role="button"
      tabindex="0"
      onclick={(e) => e.target === e.currentTarget && closeDeleteDialog()}
      onkeydown={(e) => e.key === 'Escape' && closeDeleteDialog()}
  >
      <div class="confirm-card">
          <div class="confirm-badge">{@html getIcon('trash', 16)}</div>
          <h3>{t('detail.delete_confirm_title')}</h3>
          <p>
              {#if deleteTarget}
                  {t('detail.delete_confirm_body', { title: sessionTitle(deleteTarget) })}
              {/if}
          </p>
          {#if deleteTarget}
              <div class="confirm-meta">
                  {#if showSessionIds}
                      <span>{t('common.id')}: {deleteTarget.sessionId}</span>
                  {/if}
                  <span>{t('common.provider')}: {deleteTarget.providerId}</span>
              </div>
          {/if}
          <div class="confirm-actions">
              <button class="btn-secondary" onclick={closeDeleteDialog} type="button" disabled={isDeleting}>
                  {t('actions.cancel')}
              </button>
              <button class="btn-danger" onclick={confirmDeleteSession} type="button" disabled={isDeleting}>
                  {@html getIcon('trash', 14)} {isDeleting ? t('actions.deleting') : t('actions.delete_session')}
              </button>
          </div>
      </div>
  </div>
</div>
{/if}

<style>
  /* All styles come from public/css/style.css */
</style>
