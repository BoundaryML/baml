import { useSetAtom } from 'jotai';
import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import AlgoliaSearch from './AlgoliaSearch';
import ChatBot from './ChatBot';
import { pendingQueryAtom, isChatbotOpenAtom } from './store';


// Global state for chatbot
let chatbotRoot: any = null;
let pendingQueryToSet: string | null = null;

// Function to toggle chatbot from outside React context
let toggleChatbot: (() => void) | null = null;
let openChatbot: (() => void) | null = null;

// Helper functions from original custom.js
const css = (s: string) => {
  const st = document.createElement('style');
  st.textContent = s;
  document.head.appendChild(st);
};

const whenReady = (f: () => void) =>
  document.readyState === 'loading' ? document.addEventListener('DOMContentLoaded', f) : f();

// Highlight functionality from original custom.js
function navigateToDoc(hit: any, q: string) {
  localStorage.setItem('baml-hl', JSON.stringify({ url: hit.u, text: q, sel: hit.sel }));

  fetch(hit.u, { credentials: 'same-origin' })
    .then((r) => r.text())
    .then((html) => {
      const temp = new DOMParser().parseFromString(html, 'text/html');
      const fresh = temp.querySelector('main') || temp.body;
      const live = document.querySelector('main') || document.body;
      if (fresh && live) {
        live.replaceWith(fresh);
        history.pushState({ pjax: true }, '', hit.u);
        highlightFromStore();
        window.dispatchEvent(new Event('resize'));
      }
    })
    .catch(() => window.location.assign(hit.u));
}

function highlightFromStore() {
  const data = JSON.parse(localStorage.getItem('baml-hl') || 'null');
  if (!data || !location.pathname.endsWith(data.url)) return;
  localStorage.removeItem('baml-hl');

  whenReady(() => {
    const scope =
      document.querySelector(data.sel) || document.querySelector('main') || document.body;
    if (!scope) return;
    const walker = document.createTreeWalker(scope, NodeFilter.SHOW_TEXT);
    const re = new RegExp(data.text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
    let added = 0;
    while (walker.nextNode() && added < 20) {
      const n = walker.currentNode as Text;
      if (re.test(n.textContent || '')) {
        const span = document.createElement('span');
        span.innerHTML = (n.textContent || '').replace(
          re,
          (m) => `<mark class="ai-hl">${m}</mark>`,
        );
        n.replaceWith(...span.childNodes);
        added++;
      }
    }
    const first = document.querySelector('.ai-hl');
    first?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  });
}

// Updated global CSS to integrate with Algolia search styling
css(`
/* Dynamic body padding will be handled by ChatBot component directly */
body.baml-ai-open{
  transition: padding-right .3s cubic-bezier(.4,0,.2,1);
  overflow-x: hidden;
}

/* Search result highlighting */
.ai-hl{background:#fff7a8;padding:0 2px;border-radius:4px;animation:ai-blink 1.6s ease-in-out 2;}
@keyframes ai-blink{
  0%,100%{background:#fff7a8;}
  50%{background:#ffe949;}
}
.goto-doc{color:#7c3aed;text-decoration:underline;font-weight:600;}

/* Error boundary styles */
.baml-error {
  padding: 16px;
  background: #fef2f2;
  border: 1px solid #fca5a5;
  border-radius: 8px;
  color: #dc2626;
  font-size: 14px;
}

/* Prevent text selection during resize */
body.resizing {
  user-select: none;
  cursor: ew-resize !important;
}

/* Improved scrollbar styling for chat */
.chat-scrollbar::-webkit-scrollbar {
  width: 6px;
}
.chat-scrollbar::-webkit-scrollbar-track {
  background: #f1f5f9;
}
.chat-scrollbar::-webkit-scrollbar-thumb {
  background: #cbd5e1;
  border-radius: 3px;
}
.chat-scrollbar::-webkit-scrollbar-thumb:hover {
  background: #94a3b8;
}
`);

// Main Fern Chatbot App component
function FernChatbotApp() {
  const setPendingQuery = useSetAtom(pendingQueryAtom);
  const setIsOpen = useSetAtom(isChatbotOpenAtom);

  // Handle pending queries from external calls
  useEffect(() => {
    if (pendingQueryToSet) {
      setPendingQuery(pendingQueryToSet);
      pendingQueryToSet = null;
    }
  });

  // Expose toggle functions to global scope
  useEffect(() => {
    toggleChatbot = () => {
      setIsOpen((current) => !current);
    };
    openChatbot = () => {
      setIsOpen(true);
    };
  }, [setIsOpen]);

  return <ChatBot />;
}

// Error boundary component
class ErrorBoundary extends React.Component<
  { children: React.ReactNode; fallback?: React.ReactNode },
  { hasError: boolean }
> {
  constructor(props: any) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError() {
    return { hasError: true };
  }

  componentDidCatch(error: any, errorInfo: any) {
    console.error('React component error:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        this.props.fallback || (
          <div className="baml-error">
            Something went wrong with the search interface. Please refresh the page.
          </div>
        )
      );
    }

    return this.props.children;
  }
}

// Simplified chatbot coordinator (minimal interface)
const ChatbotManager = {
  openWithQuery(query: string) {
    console.log('openWithQuery', query);
    // Set the pending query for React to pick up
    pendingQueryToSet = query;
    
    // Open the chatbot using the exposed function
    if (openChatbot) {
      openChatbot();
    }
  },

  toggle() {
    // Toggle the chatbot using the exposed function
    if (toggleChatbot) {
      toggleChatbot();
    }
  },

  initialize() {
    console.info('Initializing chatbot');
    if (chatbotRoot) {
      return; // Already initialized
    }

    try {
      // Clean up any existing instances
      const existing = document.getElementById('fern-chatbot-root');
      if (existing) {
        existing.remove();
      }

      // Create a container for the React root
      const chatbotContainer = document.createElement('div');
      chatbotContainer.id = 'fern-chatbot-root';
      document.body.appendChild(chatbotContainer);

      chatbotRoot = createRoot(chatbotContainer);

      // Render the chatbot - always mounted, visibility controlled by atom
      chatbotRoot.render(
        <ErrorBoundary fallback={<div className="baml-error">Chatbot failed to load</div>}>
            <FernChatbotApp />
        </ErrorBoundary>,
      );
    } catch (error) {
      console.error('Failed to initialize chatbot:', error);
    }
  },

  cleanup() {
    if (chatbotRoot) {
      try {
        chatbotRoot.unmount();
      } catch (error) {
        console.error('Error unmounting chatbot:', error);
      }
      chatbotRoot = null;
    }

    // Clean up the container
    const container = document.getElementById('fern-chatbot-root');
    if (container) {
      container.remove();
    }

    document.body.classList.remove('baml-ai-open');
  },
};

// Search interface root reference
let searchInterfaceRoot: any = null;

// Search interface integration with Algolia
function initializeSearchInterface() {
  let initialized = false;
  let observer: MutationObserver | null = null;

  const tryInitialize = () => {
    if (initialized) return true;

    const searchTarget = document.querySelector('[data-search], .fern-search, [class*="search"]');

    if (!searchTarget) return false;

    try {
      // Build custom search interface with Algolia integration
      const wrap = document.createElement('div');
      wrap.id = 'baml-search-wrap';
      wrap.style.cssText = 'max-width: 640px; width: 100%; position: relative;';

      const algoliaContainer = document.createElement('div');
      algoliaContainer.id = 'baml-algolia-container';
      algoliaContainer.style.cssText = 'width: 100%; position: relative;';

      wrap.append(algoliaContainer);

      // Hide original search and replace with custom
      if (searchTarget.parentNode) {
        (searchTarget as HTMLElement).style.display = 'none';
        searchTarget.parentNode.insertBefore(wrap, searchTarget);
      }

      // Render Algolia search component with error boundary
      searchInterfaceRoot = createRoot(algoliaContainer);

      searchInterfaceRoot.render(
        <ErrorBoundary fallback={<div className="baml-error">Search failed to load</div>}>
          <AlgoliaSearch onToggleChatbot={() => ChatbotManager.toggle()} />
        </ErrorBoundary>,
      );

      initialized = true;

      // Clean up observer
      if (observer) {
        observer.disconnect();
        observer = null;
      }

      return true;
    } catch (error) {
      console.error('Failed to initialize search interface:', error);
      return false;
    }
  };

  // Try immediate initialization
  if (tryInitialize()) {
    return;
  }

  // Set up observer for dynamic content
  observer = new MutationObserver(() => {
    tryInitialize();
  });

  observer.observe(document.body, {
    subtree: true,
    childList: true,
    attributes: false,
    characterData: false,
  });

  // Cleanup observer after 10 seconds to prevent memory leaks
  setTimeout(() => {
    if (observer && !initialized) {
      console.warn('Search interface not found after 10 seconds, stopping observer');
      observer.disconnect();
      observer = null;
    }
  }, 10000);
}

// Global initialization function that Fern can call
declare global {
  interface Window {
    initFernChatbot: (options?: { apiEndpoint?: string }) => void;
    navigateToDoc: (hit: { u: string; t: string; sel?: string }, q: string) => void;
    __fernChatbotCleanup?: () => void;
  }
}

// Expose navigateToDoc globally
window.navigateToDoc = navigateToDoc;

// Expose cleanup function for testing/debugging
window.__fernChatbotCleanup = () => {
  ChatbotManager.cleanup();
};

window.initFernChatbot = (options = {}) => {
  try {
    // Always initialize chatbot on page load
    ChatbotManager.initialize();
    // Initialize search interface integration
    initializeSearchInterface();

    // Handle highlight functionality
    highlightFromStore();

    // Handle navigation events
    const handlePopState = (e: PopStateEvent) => {
      if ((e.state as any)?.pjax) {
        highlightFromStore();
      }
    };

    // Clean up existing listener to prevent duplicates
    window.removeEventListener('popstate', handlePopState);
    window.addEventListener('popstate', handlePopState);

    console.log('Initialized BAML custom extensions');
  } catch (error) {
    console.error('Failed to initialize Fern chatbot:', error);
  }
};

// Auto-initialize if running in a browser environment
if (typeof window !== 'undefined') {
  whenReady(() => {
    window.initFernChatbot();
  });

  // Cleanup on page unload
  window.addEventListener('beforeunload', () => {
    if (window.__fernChatbotCleanup) {
      window.__fernChatbotCleanup();
    }
  });
}
