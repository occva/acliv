<script lang="ts">
  import { tick } from 'svelte';
  import { Marked } from 'marked';
  import hljs from 'highlight.js/lib/common';
  import powershell from 'highlight.js/lib/languages/powershell';
  import dos from 'highlight.js/lib/languages/dos';
  import DOMPurify from 'dompurify';

  let { content = "" } = $props<{ content: string }>();

  type MermaidModule = typeof import('mermaid');

  const COPY_ICON = `<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M0 6.75C0 5.784.784 5 1.75 5h1.5a.75.75 0 0 1 0 1.5h-1.5a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-1.5a.75.75 0 0 1 1.5 0v1.5A1.75 1.75 0 0 1 9.25 15h-7.5A1.75 1.75 0 0 1 0 13.25Z"/><path d="M5 1.75C5 .784 5.784 0 6.75 0h7.5C15.216 0 16 .784 16 1.75v7.5A1.75 1.75 0 0 1 14.25 11h-7.5A1.75 1.75 0 0 1 5 9.25Zm1.75-.25a.25.25 0 0 0-.25.25v7.5c0 .138.112.25.25.25h7.5a.25.25 0 0 0 .25-.25v-7.5a.25.25 0 0 0-.25-.25H1.75ZM3.5 6.25a.75.75 0 0 1 .75-.75h7a.75.75 0 0 1 0 1.5h-7a.75.75 0 0 1-.75-.75Zm.75 2.25a.75.75 0 0 0 0 1.5h4a.75.75 0 0 0 0-1.5h-4Z"/></svg>`;
  const CHECK_ICON = `<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor"><path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.22 9.28a.75.75 0 0 1 1.06-1.06L6 10.94l6.72-6.72a.75.75 0 0 1 1.06 0Z"/></svg>`;

  let container: HTMLDivElement | undefined;
  let mermaidApi: MermaidModule['default'] | null = null;
  let renderToken = 0;

  hljs.registerLanguage('powershell', powershell);
  hljs.registerLanguage('dos', dos);
  hljs.registerAliases(['ps', 'ps1', 'pwsh'], { languageName: 'powershell' });
  hljs.registerAliases(['bat', 'cmd'], { languageName: 'dos' });

  function escapeHtml(text: string): string {
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }

  function normalizeMarkdownInput(text: string): string {
    return text
      .replace(/\r\n/g, '\n')
      .replace(/\r/g, '\n')
      .replace(/\u200B/g, '')
      .replace(/^\s+(\d+\s*[→-]>?)/gm, '$1');
  }

  const markedInstance = new Marked();

  markedInstance.use({
    renderer: {
      code(code: string, lang: string | undefined) {
        const text = (code || '').trim();
        const language = (lang || '').trim();
        const normalizedLang = language.toLowerCase();

        if (normalizedLang === 'mermaid') {
          const encodedContent = encodeURIComponent(text);
          return `
            <div class="mermaid-block-wrapper">
              <div class="code-header">
                <span class="code-lang">mermaid</span>
                <button class="copy-btn"
                        type="button"
                        data-code="${encodedContent}">
                  ${COPY_ICON}
                  <span>COPY</span>
                </button>
              </div>
              <div class="mermaid-diagram" data-mermaid="${encodedContent}"></div>
            </div>
          `;
        }

        const isBlockedLang = normalizedLang === 'html' || normalizedLang === 'css';
        const isKnownLang = !!normalizedLang && hljs.getLanguage(normalizedLang);
        if (!isKnownLang || isBlockedLang) {
          return `<div>${escapeHtml(text).replace(/\n/g, '<br>')}</div>`;
        }

        let highlighted = '';
        const validLanguage = normalizedLang;

        try {
          if (text) {
            highlighted = hljs.highlight(text, { language: validLanguage }).value;
          } else {
            highlighted = escapeHtml(text);
          }
        } catch (e) {
          console.warn('[Markdown] Highlight.js error:', e);
          highlighted = escapeHtml(text);
        }

        const encodedContent = encodeURIComponent(text);

        return `
          <div class="code-block-wrapper">
            <div class="code-header">
              <span class="code-lang">${validLanguage}</span>
              <button class="copy-btn"
                      type="button"
                      data-code="${encodedContent}">
                ${COPY_ICON}
                <span>COPY</span>
              </button>
            </div>
            <pre><code class="hljs language-${validLanguage}">${highlighted || ' '}</code></pre>
          </div>
        `;
      },
      link(href: string, title: string | null | undefined, text: string) {
        return `<a href="${href}" title="${title || ''}" target="_blank" rel="noopener noreferrer">${text}</a>`;
      }
    },
    breaks: true,
    gfm: true
  });

  async function ensureMermaid() {
    if (mermaidApi) {
      return mermaidApi;
    }

    const module = await import('mermaid');
    mermaidApi = module.default;
    mermaidApi.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: 'neutral',
    });
    return mermaidApi;
  }

  async function renderMermaidDiagrams() {
    if (!container) {
      return;
    }

    const diagrams = Array.from(container.querySelectorAll<HTMLElement>('.mermaid-diagram[data-mermaid]'));
    if (!diagrams.length) {
      return;
    }

    const currentToken = ++renderToken;
    const mermaid = await ensureMermaid();
    if (currentToken !== renderToken) {
      return;
    }

    for (const [index, node] of diagrams.entries()) {
      const encoded = node.dataset.mermaid;
      if (!encoded) {
        continue;
      }

      const source = decodeURIComponent(encoded);
      const renderId = `mermaid-${Date.now()}-${currentToken}-${index}`;

      try {
        const { svg, bindFunctions } = await mermaid.render(renderId, source);
        if (currentToken !== renderToken) {
          return;
        }
        node.innerHTML = svg;
        bindFunctions?.(node);
      } catch (error) {
        console.error('[Markdown] Mermaid render error:', error);
        node.innerHTML = `
          <div class="mermaid-error">Mermaid render failed. Showing source instead.</div>
          <pre class="mermaid-fallback"><code>${escapeHtml(source)}</code></pre>
        `;
      }
    }
  }

  const htmlContent = $derived.by(() => {
    if (typeof content === 'string' && content.trim()) {
      try {
        const normalizedInput = normalizeMarkdownInput(content);
        const rawHtml = markedInstance.parse(normalizedInput) as string;
        return DOMPurify.sanitize(rawHtml, {
          ADD_ATTR: ['target', 'rel', 'data-code', 'data-mermaid'],
          ADD_TAGS: ['svg', 'path', 'button', 'span', 'foreignObject'],
        });
      } catch (e) {
        console.error('[Markdown] Parse/Purify error:', e);
        return `<div class="parse-error">${escapeHtml(content)}</div>`;
      }
    }
    return '';
  });

  $effect(() => {
    htmlContent;
    void tick().then(() => renderMermaidDiagrams());
  });

  function handleClick(event: MouseEvent) {
    const target = (event.target as HTMLElement).closest('.copy-btn');
    if (target) {
      event.preventDefault();
      event.stopPropagation();

      const btn = target as HTMLButtonElement;
      const encodedCode = btn.getAttribute('data-code') || '';
      const code = decodeURIComponent(encodedCode);

      if (!navigator.clipboard) {
        console.error('Clipboard API not available');
        return;
      }

      navigator.clipboard.writeText(code).then(() => {
        const originalHTML = btn.innerHTML;
        btn.innerHTML = `${CHECK_ICON} <span>COPIED</span>`;
        btn.classList.add('copied');

        setTimeout(() => {
          btn.innerHTML = originalHTML;
          btn.classList.remove('copied');
        }, 2000);
      }).catch(err => {
        console.error('Failed to copy text: ', err);
      });
    }
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      const target = (event.target as HTMLElement).closest('.copy-btn');
      if (target) {
        handleClick(event as unknown as MouseEvent);
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="message-content" 
     role="button" 
     tabindex="-1"
     aria-label="Markdown content"
     bind:this={container}
     onclick={handleClick}
     onkeydown={handleKeyDown}>
  {@html htmlContent}
</div>

<style>
  .message-content {
    line-height: 1.6;
    color: var(--text-primary);
  }

  .message-content :global(pre code) {
    color: #e6edf3 !important;
  }

  .message-content :global(.mermaid-block-wrapper) {
    margin: 8px 0;
    border: 1px solid var(--border-color);
    border-radius: 12px;
    overflow: hidden;
    background: color-mix(in srgb, var(--bg-secondary) 85%, transparent);
  }

  .message-content :global(.mermaid-diagram) {
    padding: 12px;
    overflow-x: auto;
    background: var(--bg-primary);
  }

  .message-content :global(.mermaid-diagram svg) {
    display: block;
    max-width: 100%;
    height: auto;
    margin: 0 auto;
  }

  .message-content :global(.mermaid-error) {
    padding: 10px 12px 0;
    color: var(--accent-red);
    font-size: 12px;
    font-weight: 600;
  }

  .message-content :global(.mermaid-fallback) {
    margin: 0;
    padding: 12px;
    overflow-x: auto;
  }
</style>
