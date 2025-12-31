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
    search: `<path d="M11.742 10.344a6.5 6.5 0 1 0-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 0 0 1.415-1.414l-3.85-3.85a1.007 1.007 0 0 0-.115-.1zM12 6.5a5.5 5.5 0 1 1-11 0 5.5 5.5 0 0 1 11 0z"/>`,
    empty_box: `<path d="M1.75 1h12.5c.966 0 1.75.784 1.75 1.75v10.5A1.75 1.75 0 0 1 14.25 15H1.75A1.75 1.75 0 0 1 0 13.25V2.75C0 1.784.784 1 1.75 1ZM1.5 2.75v10.5c0 .138.112.25.25.25h12.5a.25.25 0 0 0 .25-.25V2.75a.25.25 0 0 0-.25-.25H1.75a.25.25 0 0 0-.25.25ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"/>`,
    dropdown_arrow: `<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M12.78 6.22a.75.75 0 0 1 0 1.06l-4.25 4.25a.75.75 0 0 1-1.06 0L3.22 7.28a.75.75 0 0 1 1.06-1.06L8 9.94l3.72-3.72a.75.75 0 0 1 1.06 0Z"></path></svg>`
  };

  function getIcon(name: keyof typeof ICONS, size = 14) {
    return `<svg width="${size}" height="${size}" viewBox="0 0 16 16" fill="currentColor">${ICONS[name]}</svg>`;
  }

  // --- State (Svelte 5 Runes) ---
  let projects = $state<api.ProjectInfo[]>([]);
  let currentProject = $state<string | null>(null);
  let conversations = $state<api.ConversationSummary[]>([]);
  let currentConversation = $state<any>(null); // TODO: Define Pair type
  let stats = $state<api.Stats>({ 
    source: 'claude', projects_count: 0, conversations_count: 0, messages_count: 0, 
    conversations_loaded: 0, skipped_count: 0, load_time: 0 
  });
  let currentSource = $state(localStorage.getItem('source') || 'claude');
  const sources = ['claude', 'codex', 'gemini'];

  // UI State
  let currentView = $state('list');
  let isSearchModalOpen = $state(false);
  let searchQuery = $state('');
  let searchResults = $state<api.SearchResult[]>([]);
  let isSourceDropdownOpen = $state(false);
  let theme = $state(localStorage.getItem('theme') || 'dark');
  let isLoading = $state(false);
  let isRefreshing = $state(false);
  let showToast = $state(false);
  let toastType = $state<'syncing' | 'success'>('syncing');

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
    window.removeEventListener('keydown', handleGlobalKeydown);
  });

  async function loadData() {
    isLoading = true;
    try {
        await api.reloadData(currentSource);
        await refreshUI();
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
      showToast = true;
      
      try {
          await api.reloadData(currentSource);
          const newStats = await api.getStats(currentSource);
          stats = newStats;
          
          const projs = await api.getProjects(currentSource);
          projects = projs;

          if (currentProject) {
              const res = await api.getConversations(currentSource, currentProject);
              conversations = res || [];
          }
          
          toastType = 'success';
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

  async function refreshUI() {
    stats = await api.getStats(currentSource);
    projects = await api.getProjects(currentSource);
    
    if (!currentProject && projects.length > 0) {
        selectProject(projects[0].name);
    }
  }

  async function selectProject(name: string) {
    currentProject = name;
    const res = await api.getConversations(currentSource, name);
    conversations = res || [];
    currentView = 'list';
  }

  interface MessagePair {
      user?: string;
      assistant?: string;
  }

  function transformConversation(conv: api.Conversation | null) {
      if (!conv) return null;
      const messages = conv.messages || [];
      const pairs: MessagePair[] = [];
      
      let i = 0;
      while (i < messages.length) {
          const msg = messages[i];
          const role = (msg.role || '').toLowerCase();
          
          if (role === 'user' || role === 'human') {
              let userContent = msg.content || '';
              // 合并连续的 user 消息
              while (i + 1 < messages.length && 
                     (messages[i+1].role.toLowerCase() === 'user' || messages[i+1].role.toLowerCase() === 'human')) {
                  const nextContent = messages[i+1].content || '';
                  if (userContent.trim().endsWith('```') && nextContent.trim().startsWith('```')) {
                      userContent = userContent.trim().slice(0, -3) + '\n' + nextContent.trim().slice(3);
                  } else {
                      userContent += '\n' + nextContent;
                  }
                  i++;
              }
              
              let assistantContent = '';
              if (i + 1 < messages.length && messages[i+1].role.toLowerCase() === 'assistant') {
                  assistantContent = messages[i+1].content || '';
                  i++;
                  while (i + 1 < messages.length && messages[i+1].role.toLowerCase() === 'assistant') {
                      const nextContent = messages[i+1].content || '';
                      const trimmedPrev = assistantContent.trim();
                      const trimmedNext = nextContent.trim();
                      
                      if (trimmedPrev.endsWith('```') && trimmedNext.startsWith('```')) {
                          assistantContent = trimmedPrev.slice(0, -3) + '\n' + trimmedNext.slice(3);
                      } else {
                          assistantContent += '\n' + nextContent;
                      }
                      i++;
                  }
              }
              
              pairs.push({ user: userContent, assistant: assistantContent });
          } else if (role === 'assistant') {
              let assistantContent = msg.content || '';
              while (i + 1 < messages.length && messages[i+1].role.toLowerCase() === 'assistant') {
                  const nextContent = messages[i+1].content || '';
                  if (assistantContent.trim().endsWith('```') && nextContent.trim().startsWith('```')) {
                      assistantContent = assistantContent.trim().slice(0, -3) + '\n' + nextContent.trim().slice(3);
                  } else {
                      assistantContent += '\n' + nextContent;
                  }
                  i++;
              }
              pairs.push({ assistant: assistantContent });
          }
          i++;
      }
      
      return { ...conv, pairs };
  }

  async function selectConversation(sessionId: string) {
      if (!currentProject) return;
      const conv = await api.getConversationDetail(currentSource, currentProject, sessionId);
      currentConversation = transformConversation(conv);
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
      loadData();
  }

  async function handleSearchInput() {
      if (!searchQuery) {
          searchResults = [];
          return;
      }
      if (searchTimer) clearTimeout(searchTimer);
      searchTimer = setTimeout(async () => {
          const res = await api.search(currentSource, searchQuery);
          searchResults = res || [];
      }, 300);
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

  function handleSearchResultClick(result: api.SearchResult) {
      closeSearch();
      if (currentProject !== result.project) {
          currentProject = result.project;
      }
      api.getConversationDetail(currentSource, result.project, result.session_id)
        .then((conv) => {
            currentConversation = transformConversation(conv);
            currentView = 'detail';
        });
  }

  function handleModalBackdropClick(e: MouseEvent) {
      if (e.target === e.currentTarget) {
          closeSearch();
      }
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
      // 正在输入时禁用某些热键
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
      'gemini': 'Gemini CLI'
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
                    {src === 'claude' ? 'Claude CLI' : src === 'codex' ? 'Codex CLI' : 'Gemini CLI'}
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
      <div class="header-actions">
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

     <div class="view" class:active={currentView === 'list'} id="listView">
         <div class="view-header">
             <h2>{currentProject || 'Select a Project'}</h2>
             {#if projects.length > 0 && currentProject}
                <span class="view-info">{conversations.length} conversations</span>
             {/if}
         </div>
         <div class="conversations-list" id="conversationsList">
            {#if conversations.length === 0}
               <div class="empty-state">
                   {@html ICONS.empty_box}
                   <h3>No conversations</h3>
               </div>
            {:else}
               {#each conversations as conv}
                   <button class="conversation-item" onclick={() => selectConversation(conv.session_id)} type="button">
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
                 ← Back
             </button>
             <h2>{currentConversation?.title || 'Conversation'}</h2>
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
      <div class="refresh-content" class:syncing={toastType === 'syncing'} class:success={toastType === 'success'}>
          {#if toastType === 'syncing'}
              <svg class="spinner-small" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M21 12a9 9 0 1 1-6.219-8.56"></path>
              </svg>
              <span>Syncing history...</span>
          {:else}
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M20 6 9 17 4 12"></path>
              </svg>
              <span>History Updated</span>
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
           <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div class="search-input-wrapper" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
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
</div>

<style>
  /* All styles come from public/css/style.css */
</style>
