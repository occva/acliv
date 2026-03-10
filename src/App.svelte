<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import * as api from './lib/api';
  import Markdown from './lib/components/Markdown.svelte';

  // --- Icons from Legacy app.js ---
  const ICONS = {
    project: `<path d="M1.75 1A1.75 1.75 0 0 0 0 2.75v10.5C0 14.216.784 15 1.75 15h12.5A1.75 1.75 0 0 0 16 13.25v-8.5A1.75 1.75 0 0 0 14.25 3H7.5a.25.25 0 0 1-.2-.1l-.9-1.2C6.07 1.22 5.55 1 5 1H1.75Z"/>`,
    conversation: `<path fill-rule="evenodd" d="M1.75 2.5a.75.75 0 0 0 0 1.5h10.5a.75.75 0 0 0 0-1.5H1.75Zm0 5a.75.75 0 0 0 0 1.5h6a.75.75 0 0 0 0-1.5h-6ZM.5 15.5l3-3h10.75a1.75 1.75 0 0 0 1.75-1.75v-9A1.75 1.75 0 0 0 14.25 0H1.75A1.75 1.75 0 0 0 0 1.75v13.75Z"/>`,
    message: `<path fill-rule="evenodd" d="M0 3.75C0 2.784.784 2 1.75 2h12.5c.966 0 1.75.784 1.75 1.75v8.5A1.75 1.75 0 0 1 14.25 14H1.75A1.75 1.75 0 0 1 0 12.25v-8.5Zm1.75-.25a.25.25 0 0 0-.25.25v8.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25v-8.5a.25.25 0 0 0-.25-.25H1.75ZM3.5 6.25a.75.75 0 0 1 .75-.75h7a.75.75 0 0 1 0 1.5h-7a.75.75 0 0 1-.75-.75Zm.75 2.25a.75.75 0 0 0 0 1.5h4a.75.75 0 0 0 0-1.5h-4Z"/>`,
    error: `<path fill-rule="evenodd" d="M8.22 1.754a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368L7.78 1.754ZM10.5 1.5a1.75 1.75 0 0 0-3 0L1.418 12.875A1.75 1.75 0 0 0 2.918 15h10.164a1.75 1.75 0 0 0 1.5-2.125L8.78 1.754ZM9 10.25a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0v2.5Zm-.75 3.25a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z"/>`,
    calendar: `<path d="M4.75 0a.75.75 0 0 1 .75.75V2h5V.75a.75.75 0 0 1 1.5 0V2h1.25c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0 1 14.25 16H1.75A1.75 1.75 0 0 1 0 14.25V3.75C0 2.784.784 2 1.75 2H3V.75A.75.75 0 0 1 3.75 0h1ZM1.5 3.75v10.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25V3.75a.25.25 0 0 0-.25-.25H1.75a.25.25 0 0 0-.25.25Z"/><path d="M4 7h2v2H4V7zm4 0h2v2H8V7z"/>`,
    trash: `<path d="M6.5 1a1 1 0 0 0-.894.553L5.382 2H2.75a.75.75 0 0 0 0 1.5h.45l.632 9.49A1.75 1.75 0 0 0 5.577 14.5h4.846a1.75 1.75 0 0 0 1.745-1.51L12.8 3.5h.45a.75.75 0 0 0 0-1.5h-2.632l-.224-.447A1 1 0 0 0 9.5 1h-3Zm.276 1.5h2.448l.25.5h-2.948l.25-.5Zm-1.44 1.999h5.328l-.617 9.255a.25.25 0 0 1-.249.216H5.202a.25.25 0 0 1-.249-.216L4.336 4.5Zm1.414 1.25a.75.75 0 0 1 .75.75v4a.75.75 0 0 1-1.5 0v-4a.75.75 0 0 1 .75-.75Zm2.5 0a.75.75 0 0 1 .75.75v4a.75.75 0 0 1-1.5 0v-4a.75.75 0 0 1 .75-.75Z"/>`,
    search: `<path d="M11.742 10.344a6.5 6.5 0 1 0-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 0 0 1.415-1.414l-3.85-3.85a1.007 1.007 0 0 0-.115-.1zM12 6.5a5.5 5.5 0 1 1-11 0 5.5 5.5 0 0 1 11 0z"/>`,
    empty_box: `<path d="M1.75 1h12.5c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0 1 14.25 15H1.75A1.75 1.75 0 0 1 0 13.25V2.75C0 1.784.784 1 1.75 1ZM1.5 2.75v10.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25V2.75a.25.25 0 0 0-.25-.25H1.75a.25.25 0 0 0-.25.25ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"/>`,
    back: `<svg viewBox="0 0 1024 1024" width="14" height="14" fill="currentColor"><path d="M604.8 407.68H158.72L375.68 198.4c17.92-17.28 17.92-46.08 0-63.36a48.384 48.384 0 0 0-65.92 0L13.44 421.12c-17.92 17.28-17.92 46.08 0 63.36l296.32 286.08c17.92 17.28 47.36 17.28 65.92 0 17.92-17.28 17.92-46.08 0-63.36L158.72 497.92h446.08c179.84 0 325.76 140.8 325.76 314.88v44.8c0 24.96 21.12 44.8 46.72 44.8 25.6 0 46.72-20.48 46.72-44.8v-44.8c0-224-187.52-405.12-419.2-405.12z"></path></svg>`,
    dropdown_arrow: `<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M12.78 6.22a.75.75 0 0 1 0 1.06l-4.25 4.25a.75.75 0 0 1-1.06 0L3.22 7.28a.75.75 0 0 1 1.06-1.06L8 9.94l3.72-3.72a.75.75 0 0 1 1.06 0Z"></path></svg>`
  };

  function getIcon(name: keyof typeof ICONS, size = 14) {
    return `<svg width="${size}" height="${size}" viewBox="0 0 16 16" fill="currentColor">${ICONS[name]}</svg>`;
  }

  // ---- 本地类型（与旧 api.ts 同构，模板字段名不变）----
  interface SessionMeta {
    providerId: string; sessionId: string; title?: string; summary?: string;
    projectDir?: string | null; lastActiveAt?: number; sourcePath?: string; resumeCommand?: string;
  }
  interface ProjectInfo { name: string; conversation_count: number; latest_date: string; }
  interface ConvSummary {
    session_id: string; project_path: string; source_type: string;
    title: string; timestamp: string; message_count: number; date: string;
  }
  interface Stats { projects_count: number; conversations_count: number; messages_count: number; }
  interface SearchResultLocal { project: string; source_type: string; session_id: string; title: string; date: string; }
  interface ConversationLike {
      session_id: string;
      project_path: string;
      source_type: string;
      title: string;
      timestamp: string;
      messages: Array<{ role: string; content: string; timestamp: string }>;
  }

  // ---- 适配函数 ----
  const GEMINI_GROUP = 'Gemini Sessions';
  const PROVIDER_GROUPS: Record<string, string> = {
    gemini: GEMINI_GROUP,
    codex: 'Codex Sessions',
    openclaw: 'OpenClaw Sessions',
    opencode: 'OpenCode Sessions',
    claude: 'Claude Sessions',
  };

  function msToDate(ms?: number): string {
    return ms ? new Date(ms).toLocaleDateString() : 'N/A';
  }
  function sessionTitle(s: SessionMeta): string {
    return s.title ?? s.summary ?? s.sessionId.slice(0, 8);
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
      name, conversation_count: count, latest_date: msToDate(latest),
    })).sort((a,b) => b.conversation_count - a.conversation_count);
  }

  // SessionMeta[] → ConvSummary[]（按 source + projectDir 过滤）
  function buildConversations(sessions: SessionMeta[], source: string, project: string): ConvSummary[] {
    return sessions
      .filter(s => s.providerId === source && sessionDir(s) === project)
      .map(s => ({
        session_id: s.sessionId, project_path: s.projectDir ?? '',
        source_type: s.providerId, title: sessionTitle(s),
        timestamp: msToDate(s.lastActiveAt), message_count: sessionMessageCount(s),
        date: msToDate(s.lastActiveAt),
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
    if (!currentProject && projs.length > 0) selectProject(projs[0].name);
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
  let searchQuery = $state('');
  let searchResults = $state<SearchResultLocal[]>([]);
  let isSourceDropdownOpen = $state(false);
  let theme = $state(localStorage.getItem('theme') || 'dark');
  let isLoading = $state(false);
  let isRefreshing = $state(false);
  let isDeleting = $state(false);
  let showToast = $state(false);
  let toastType = $state<'syncing' | 'success' | 'error'>('syncing');
  let toastMessage = $state('History Updated');
  let deleteTarget = $state<SessionMeta | null>(null);

  // Timers
  let autoRefreshInterval: any;
  let searchTimer: any;

  onMount(async () => {
    setTheme(theme);
    await loadData();
    autoRefreshInterval = setInterval(silentRefresh, 120000);
    window.addEventListener('keydown', handleGlobalKeydown);
  });

  onDestroy(() => {
    if (autoRefreshInterval) clearInterval(autoRefreshInterval);
    if (searchTimer) clearTimeout(searchTimer);
    countJobToken++;
    window.removeEventListener('keydown', handleGlobalKeydown);
  });
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
          const list = await api.getSessionMessages(s.providerId, s.sourcePath!);
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
        allSessions = await api.listSessions();
        refreshFromSessions();
    } catch (e) {
        console.error("Failed to load data:", e);
    } finally {
        isLoading = false;
    }
  }
  async function silentRefresh() {
      if (isLoading || isRefreshing) return;
      isRefreshing = true;
      toastType = 'syncing';
      toastMessage = 'Syncing history...';
      showToast = true;
      
      try {
          allSessions = await api.listSessions();
          refreshFromSessions();
          if (currentProject) {
              conversations = buildConversations(allSessions, currentSource, currentProject);
              void warmupMessageCounts(currentProject);
          }
          
          toastType = 'success';
          toastMessage = 'History Updated';
          setTimeout(() => {
              showToast = false;
              isRefreshing = false;
          }, 3000);
      } catch(e) { 
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
          currentView = 'list';

          allSessions = await api.listSessions();

          const projectStillExists = allSessions.some(
              s => s.providerId === currentSource && sessionDir(s) === deletedProject,
          );
          currentProject = projectStillExists ? deletedProject : null;
          conversations = currentProject
              ? buildConversations(allSessions, currentSource, currentProject)
              : [];

          refreshFromSessions();

          if (currentProject) {
              conversations = buildConversations(allSessions, currentSource, currentProject);
              void warmupMessageCounts(currentProject);
          }

          toastType = 'success';
          toastMessage = 'Session deleted';
          showToast = true;
          setTimeout(() => {
              showToast = false;
          }, 3000);
      } catch (e) {
          console.error('Failed to delete session:', e);
          toastType = 'error';
          toastMessage = 'Delete failed';
          showToast = true;
          setTimeout(() => {
              showToast = false;
          }, 3000);
      } finally {
          isDeleting = false;
      }
  }
  function selectProject(name: string) {
    currentProject = name;
    conversations = buildConversations(allSessions, currentSource, name);
    currentView = 'list';
    void warmupMessageCounts(name);
  }

  interface MessagePair {
      user?: string;
      assistant?: string;
  }
  function mergeMessageContent(current: string, next: string) {
      if (!current) return next;
      if (!next) return current;
      const needsSingleNewline = current.endsWith('\n') || next.startsWith('\n');
      return `${current}${needsSingleNewline ? '\n' : '\n\n'}${next}`;
  }

  function transformConversation(conv: ConversationLike | null) {
      if (!conv) return null;
      const messages = conv.messages || [];
      const pairs: MessagePair[] = [];

      let i = 0;
      while (i < messages.length) {
          const msg = messages[i];
          const role = (msg.role || '').toLowerCase();

          if (role === 'user' || role === 'human') {
              let userContent = msg.content || '';
              while (i + 1 < messages.length &&
                     (messages[i+1].role.toLowerCase() === 'user' || messages[i+1].role.toLowerCase() === 'human')) {
                  const nextContent = messages[i+1].content || '';
                  userContent = mergeMessageContent(userContent, nextContent);
                  i++;
              }
              let assistantContent = '';
              if (i + 1 < messages.length && messages[i+1].role.toLowerCase() === 'assistant') {
                  assistantContent = messages[i+1].content || '';
                  i++;
                  while (i + 1 < messages.length && messages[i+1].role.toLowerCase() === 'assistant') {
                      const nextContent = messages[i+1].content || '';
                      assistantContent = mergeMessageContent(assistantContent, nextContent);
                      i++;
                  }
              }
              pairs.push({ user: userContent, assistant: assistantContent });
          } else if (role === 'assistant') {
              let assistantContent = msg.content || '';
              while (i + 1 < messages.length && messages[i+1].role.toLowerCase() === 'assistant') {
                  const nextContent = messages[i+1].content || '';
                  assistantContent = mergeMessageContent(assistantContent, nextContent);
                  i++;
              }
              pairs.push({ assistant: assistantContent });
          }
          i++;
      }
      return { ...conv, pairs };
  }
  async function selectConversation(sessionId: string, sourceType?: string) {
      if (!currentProject) return;
      const target = allSessions.find(s => s.sessionId === sessionId && (!sourceType || s.providerId === sourceType));
      if (!target) return;
      const rawMsgs = target.sourcePath
        ? await api.getSessionMessages(target.providerId, target.sourcePath)
        : [];
      messageCountCache = {
        ...messageCountCache,
        [sessionCacheKey(target)]: rawMsgs.length,
      };
      const convLike = {
        session_id: target.sessionId,
        project_path: target.projectDir ?? '',
        source_type: target.providerId,
        title: sessionTitle(target),
        timestamp: msToDate(target.lastActiveAt),
        messages: rawMsgs.map(m => ({
          role: m.role, content: m.content, timestamp: msToDate(m.ts),
        })),
      };
      currentConversation = transformConversation(convLike as any);
      currentView = 'detail';
  }

  function setTheme(newTheme: string) {
      theme = newTheme;
      document.documentElement.setAttribute('data-theme', theme);
      localStorage.setItem('theme', theme);
  }

  function toggleTheme() {
      setTheme(theme === 'dark' ? 'light' : 'dark');
  }
  function selectSource(source: string) {
      if (currentSource === source) {
          isSourceDropdownOpen = false;
          return;
      }
      currentSource = source;
      localStorage.setItem('source', source);
      isSourceDropdownOpen = false;
      currentProject = null;
      currentConversation = null;
      refreshFromSessions();
  }
  async function handleSearchInput() {
      if (!searchQuery) {
          searchResults = [];
          return;
      }
      const needle = searchQuery.toLowerCase();
      searchResults = allSessions
        .filter(s => s.providerId === currentSource
          && [s.sessionId, s.title, s.summary, s.projectDir]
               .some(f => f?.toLowerCase().includes(needle)))
        .map(s => ({
          project: sessionDir(s),
          source_type: s.providerId,
          session_id: s.sessionId,
          title: sessionTitle(s),
          date: msToDate(s.lastActiveAt),
        }))
        .slice(0, 50) as any;
  }

  function openSearch() {
      isSearchModalOpen = true;
      setTimeout(() => document.getElementById('searchInput')?.focus(), 50);
  }

  function closeSearch() {
      isSearchModalOpen = false;
      searchQuery = '';
      searchResults = [];
  }

  function handleSearchResultClick(result: any) {
      closeSearch();
      if (currentProject !== result.project) {
          currentProject = result.project;
      }
      selectConversation(result.session_id, result.source_type);
  }

  function handleModalBackdropClick(e: MouseEvent) {
      if (e.target === e.currentTarget) {
          closeSearch();
      }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
      // Disable global hotkeys while user is typing.
      if (document.activeElement?.tagName === 'INPUT' || document.activeElement?.tagName === 'TEXTAREA') {
          if (e.key === 'Escape' && isSearchModalOpen) closeSearch();
          return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
          e.preventDefault();
          openSearch();
      }
      if (e.key === 'Escape') {
          if (isSearchModalOpen) closeSearch();
          else if (currentView === 'detail') currentView = 'list';
      }
      
      if (!isSearchModalOpen && currentView === 'list' && projects.length > 0) {
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

  const sourceLabel = $derived(({
      'claude': 'Claude CLI',
      'codex': 'Codex CLI',
      'gemini': 'Gemini CLI',
      'openclaw': 'OpenClaw',
      'opencode': 'OpenCode'
  } as Record<string, string>)[currentSource] || 'History');

</script>

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
                     onclick={() => selectProject(project.name)}
                     type="button">
                    <span class="project-name">{project.name}</span>
                    <span class="project-count">{project.conversation_count}</span>
                </button>
            {/each}
        </div>
    </div>
  </aside>

  <main class="main-content">
     <div class="view" class:active={currentView === 'list'} id="listView">
         <div class="view-header">
             <h2>{currentProject || 'Select a Project'}</h2>
             {#if projects.length > 0 && currentProject}
                <span class="view-info">{conversations.length} conversations</span>
             {/if}
             <div class="view-header-actions">
                 <button class="action-btn" id="openSearchBtn" onclick={openSearch} type="button">
                    {@html getIcon('search', 16)}
                 </button>
                 <button class="action-btn theme-toggle" id="themeToggle" onclick={toggleTheme} type="button">
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
                   <h3>No conversations</h3>
               </div>
            {:else}
               {#each conversations as conv}
                   <button class="conversation-item" onclick={() => selectConversation(conv.session_id, conv.source_type)} type="button">
                       <div class="conversation-title">{conv.title}</div>
                       <div class="conversation-meta">
                           <span class="meta-item">{@html getIcon('conversation', 12)} {conv.message_count} messages</span>
                           <span class="meta-item">{@html getIcon('calendar', 12)} {conv.date}</span>
                       </div>
                   </button>
               {/each}
            {/if}
         </div>
     </div>

     <div class="view" class:active={currentView === 'detail'} id="detailView">
        <div class="view-header">
             <button class="btn-secondary" id="backBtn" onclick={() => currentView = 'list'} type="button">
                 {@html ICONS.back} Back
             </button>
             <h2>{currentConversation?.title || 'Conversation'}</h2>
             {#if currentConversation}
                 <div class="view-header-actions">
                     <button class="action-btn" onclick={openSearch} type="button">
                        {@html getIcon('search', 16)}
                     </button>
                     <button class="action-btn theme-toggle" onclick={toggleTheme} type="button">
                         {#if theme === 'light'}
                           <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M9.598 1.591a.75.75 0 01.785-.175 7 7 0 11-8.967 8.967.75.75 0 01.961-.96 5.5 5.5 0 007.046-7.046.75.75 0 01.175-.786zm1.616 1.945a7 7 0 01-7.678 7.678 5.5 5.5 0 107.678-7.678z"></path></svg>
                         {:else}
                           <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 12a4 4 0 100-8 4 4 0 000 8zM8 0a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 0zm0 13a.5.5 0 01.5.5v2a.5.5 0 01-1 0v-2A.5.5 0 018 13zM2.343 2.343a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L2.343 3.05a.5.5 0 010-.707zm11.314 8.486a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM12.914 2.343a.5.5 0 010 .707l-1.414 1.414a.5.5 0 01-.707-.707l1.414-1.414a.5.5 0 01.707 0zM3.05 12.207a.5.5 0 01.707 0l1.414 1.414a.5.5 0 01-.707.707L3.05 12.914a.5.5 0 010-.707zM13 8a.5.5 0 01.5.5h2a.5.5 0 010-1h-2A.5.5 0 0113 8zM0 8a.5.5 0 01.5-.5h2a.5.5 0 010 1h-2A.5.5 0 010 8z"></path></svg>
                         {/if}
                     </button>
                     <button class="btn-danger" onclick={openDeleteDialog} type="button" disabled={isDeleting}>
                         {@html getIcon('trash', 14)} {isDeleting ? 'Deleting...' : 'Delete'}
                     </button>
                 </div>
             {/if}
        </div>
        <div class="conversation-detail" id="conversationDetail">
            {#if currentConversation}
                <div class="conversation-header">
                    <h3>{currentConversation.title}</h3>
                     <div class="conversation-info">
                        <span>{@html getIcon('message', 12)} ID: {currentConversation.session_id}</span>
                        <span>{@html getIcon('calendar', 12)} {currentConversation.timestamp || 'N/A'}</span>
                    </div>
                </div>
                <div class="messages-container">
                    {#each currentConversation.pairs as pair, i}
                        {#if pair.user}
                            <div class="message user-message">
                                <div class="message-header">
                                    <span class="message-role">User</span>
                                    <span class="message-number">#{i + 1}</span>
                                </div>
                                <Markdown content={pair.user} />
                            </div>
                        {/if}
                        {#if pair.assistant}
                            <div class="message assistant-message">
                                <div class="message-header">
                                    <span class="message-role">Assistant</span>
                                    {#if pair.user}<span class="message-number">#{i+1}</span>{/if}
                                </div>
                                <Markdown content={pair.assistant} />
                            </div>
                        {/if}
                    {/each}
                </div>
            {/if}
        </div>
     </div>

  <div class="refresh-toast" class:show={showToast}>
      <div class="refresh-content" class:syncing={toastType === 'syncing'} class:success={toastType === 'success'} class:error={toastType === 'error'}>
          {#if toastType === 'syncing'}
              <svg class="spinner-small" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
              </svg>
              <span>{toastMessage}</span>
          {:else}
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M20 6 9 17 4 12"></path>
              </svg>
              <span>{toastMessage}</span>
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
              <input type="text" id="searchInput" placeholder="Search conversations..." 
                     bind:value={searchQuery} 
                     oninput={handleSearchInput} />
              <button class="btn-close-search" onclick={closeSearch} type="button">ESC</button>
          </div>
          <div class="search-modal-results" id="searchModalResults">
              {#each searchResults as result}
                  <button class="conversation-item" onclick={() => handleSearchResultClick(result)} type="button">
                      <div class="conversation-title">{result.title}</div>
                       <div class="conversation-meta">
                            <span class="meta-item">{@html getIcon('project', 12)} {result.project}</span>
                            <span class="meta-item">{@html getIcon('calendar', 12)} {result.date}</span>
                       </div>
                  </button>
              {/each}
          </div>
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
          <h3>Delete session?</h3>
          <p>
              {#if deleteTarget}
                  This will permanently remove <strong>{sessionTitle(deleteTarget)}</strong> and its provider-side session files.
              {/if}
          </p>
          {#if deleteTarget}
              <div class="confirm-meta">
                  <span>ID: {deleteTarget.sessionId}</span>
                  <span>Provider: {deleteTarget.providerId}</span>
              </div>
          {/if}
          <div class="confirm-actions">
              <button class="btn-secondary" onclick={closeDeleteDialog} type="button" disabled={isDeleting}>
                  Cancel
              </button>
              <button class="btn-danger" onclick={confirmDeleteSession} type="button" disabled={isDeleting}>
                  {@html getIcon('trash', 14)} {isDeleting ? 'Deleting...' : 'Delete Session'}
              </button>
          </div>
      </div>
  </div>
</div>

<style>
  /* All styles come from public/css/style.css */
</style>

