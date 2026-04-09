import React, { useRef, useEffect, useState, useCallback } from 'react';
import ReactMarkdown, { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Highlight, themes } from 'prism-react-renderer';
import { X, Trash2, Send, GripVertical, Square, Copy, Check } from 'lucide-react';
import { useChat } from './hooks/useChat';
import { trackCitationClick, trackSuggestionClick } from '../../lib/analytics';
import { Feedback } from './Feedback';
import styles from './AskBamlSidePanel.module.css';

// Hook to detect dark mode from DOM (works without ColorModeProvider)
function useIsDarkMode(): boolean {
  const [isDarkMode, setIsDarkMode] = useState(() => {
    if (typeof document !== 'undefined') {
      return document.documentElement.getAttribute('data-theme') === 'dark';
    }
    return false;
  });

  useEffect(() => {
    if (typeof document === 'undefined') return;

    const observer = new MutationObserver((mutations) => {
      mutations.forEach((mutation) => {
        if (mutation.attributeName === 'data-theme') {
          setIsDarkMode(document.documentElement.getAttribute('data-theme') === 'dark');
        }
      });
    });

    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });

    // Initial check
    setIsDarkMode(document.documentElement.getAttribute('data-theme') === 'dark');

    return () => observer.disconnect();
  }, []);

  return isDarkMode;
}

// Map language aliases
const languageMap: Record<string, string> = {
  js: 'javascript',
  ts: 'typescript',
  py: 'python',
  rb: 'ruby',
  sh: 'bash',
  shell: 'bash',
  baml: 'typescript', // Use typescript highlighting for BAML as closest match
};

function normalizeLanguage(lang: string): string {
  return languageMap[lang.toLowerCase()] || lang.toLowerCase() || 'typescript';
}

// Code block with syntax highlighting and copy button
function CodeBlock({ children, className, isDarkMode }: { children: React.ReactNode; className?: string; isDarkMode: boolean }) {
  const [copied, setCopied] = useState(false);
  
  // Extract code text from children
  const codeText = typeof children === 'string' 
    ? children 
    : React.Children.toArray(children).map(child => 
        typeof child === 'string' ? child : ''
      ).join('');

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(codeText).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [codeText]);

  // Determine the language from className (e.g., "language-baml")
  const rawLanguage = className?.replace(/^language-/, '') || '';
  const displayLanguage = rawLanguage.toUpperCase() || 'CODE';
  const prismLanguage = normalizeLanguage(rawLanguage);

  // Select theme based on color mode - use themes that match Docusaurus
  const prismTheme = isDarkMode ? themes.vsDark : themes.github;

  return (
    <div className={styles.codeBlockWrapper}>
      <div className={styles.codeBlockHeader}>
        <span className={styles.codeBlockLanguage}>{displayLanguage}</span>
        <button
          onClick={handleCopy}
          className={styles.copyButton}
          title={copied ? 'Copied!' : 'Copy code'}
          type="button"
        >
          {copied ? (
            <>
              <Check size={14} strokeWidth={2} />
              <span>Copied</span>
            </>
          ) : (
            <>
              <Copy size={14} strokeWidth={2} />
              <span>Copy</span>
            </>
          )}
        </button>
      </div>
      <Highlight
        theme={prismTheme}
        code={codeText.trim()}
        language={prismLanguage}
      >
        {({ className: highlightClassName, style, tokens, getLineProps, getTokenProps }) => (
          <pre 
            className={`${styles.codeBlock} ${highlightClassName}`} 
            style={{ ...style, background: 'transparent' }}
          >
            {tokens.map((line, i) => (
              <div key={i} {...getLineProps({ line })}>
                {line.map((token, key) => (
                  <span key={key} {...getTokenProps({ token })} />
                ))}
              </div>
            ))}
          </pre>
        )}
      </Highlight>
    </div>
  );
}

// Factory function to create markdown components with dark mode awareness
function createMarkdownComponents(isDarkMode: boolean): Components {
  return {
    code({ className, children }) {
      // Check if this is inline code (no className) or block code (has className like "language-xxx")
      // For block code inside pre, we just render the code element - pre handles the wrapper
      const isInline = !className;
      if (isInline) {
        return <code className={styles.inlineCode}>{children}</code>;
      }
      // Block code - render plain code element, pre wrapper handles the rest
      return <code className={className}>{children}</code>;
    },
    pre({ children }) {
      // Extract code content and language from the children
      // children could be a React element (our custom code component) or raw element
      const childArray = React.Children.toArray(children);
      
      // Find the code element - it might be a custom component or a raw element
      let codeContent: React.ReactNode = '';
      let language = '';
      
      for (const child of childArray) {
        if (React.isValidElement(child)) {
          const childProps = child.props as { className?: string; children?: React.ReactNode };
          // Get the className which contains the language
          if (childProps.className) {
            language = childProps.className.replace(/^language-/, '');
          }
          // Get the actual code content
          codeContent = childProps.children;
          break;
        }
      }
      
      // If we couldn't extract, just render children directly
      if (!codeContent && childArray.length > 0) {
        codeContent = children;
      }
      
      return <CodeBlock className={language ? `language-${language}` : ''} isDarkMode={isDarkMode}>{codeContent}</CodeBlock>;
    },
  p({ children }) {
    return <p className={styles.markdownParagraph}>{children}</p>;
  },
  ul({ children }) {
    return <ul className={styles.markdownList}>{children}</ul>;
  },
  ol({ children }) {
    return <ol className={styles.markdownOrderedList}>{children}</ol>;
  },
  li({ children }) {
    return <li className={styles.markdownListItem}>{children}</li>;
  },
  a({ href, children }) {
    return (
      <a href={href} target="_blank" rel="noopener noreferrer" className={styles.markdownLink}>
        {children}
      </a>
    );
  },
  blockquote({ children }) {
    return <blockquote className={styles.markdownBlockquote}>{children}</blockquote>;
  },
  h1({ children }) {
    return <h1 className={styles.markdownHeading1}>{children}</h1>;
  },
  h2({ children }) {
    return <h2 className={styles.markdownHeading2}>{children}</h2>;
  },
  h3({ children }) {
    return <h3 className={styles.markdownHeading3}>{children}</h3>;
  },
  table({ children }) {
    return <div className={styles.markdownTableWrapper}><table className={styles.markdownTable}>{children}</table></div>;
  },
  th({ children }) {
    return <th className={styles.markdownTh}>{children}</th>;
  },
    td({ children }) {
      return <td className={styles.markdownTd}>{children}</td>;
    },
  };
}

const MIN_PANEL_WIDTH = 280;
const MAX_PANEL_WIDTH = 600;
const DEFAULT_PANEL_WIDTH = 380;

export function AskBamlSidePanel() {
  const { messages, isLoading, isOpen, setIsOpen, sendMessage, clearMessages, streamingMessageId, stopStreaming } = useChat();
  const isDarkMode = useIsDarkMode();
  const [input, setInput] = useState('');
  const [panelWidth, setPanelWidth] = useState(DEFAULT_PANEL_WIDTH);
  const [isResizing, setIsResizing] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  
  // Create markdown components with current color mode
  const markdownComponents = React.useMemo(() => createMarkdownComponents(isDarkMode), [isDarkMode]);

  // Auto-grow textarea
  const adjustTextareaHeight = useCallback(() => {
    const textarea = inputRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      const newHeight = Math.min(textarea.scrollHeight, 150); // Max height of 150px
      textarea.style.height = `${newHeight}px`;
    }
  }, []);

  useEffect(() => {
    adjustTextareaHeight();
  }, [input, adjustTextareaHeight]);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Focus input when panel opens
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => inputRef.current?.focus(), 100);
    }
  }, [isOpen]);

  // Handle resize
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  }, []);

  useEffect(() => {
    if (!isResizing) return;

    const handleMouseMove = (e: MouseEvent) => {
      const newWidth = window.innerWidth - e.clientX;
      setPanelWidth(Math.min(MAX_PANEL_WIDTH, Math.max(MIN_PANEL_WIDTH, newWidth)));
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [isResizing]);

  // Add body class when panel is open to adjust main content
  useEffect(() => {
    if (isOpen) {
      document.documentElement.style.setProperty('--ask-panel-width', `${panelWidth}px`);
      document.body.classList.add('ask-panel-open');
    } else {
      document.body.classList.remove('ask-panel-open');
    }

    return () => {
      document.body.classList.remove('ask-panel-open');
    };
  }, [isOpen, panelWidth]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (input.trim()) {
      sendMessage(input);
      setInput('');
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  // Get the last user message for tracking
  const lastUserMessage = messages.filter(m => m.role === 'user').pop();
  const lastMessage = messages[messages.length - 1];

  const handleCitationClick = (cite: { title: string; url: string }) => {
    if (lastUserMessage) {
      trackCitationClick({
        citationUrl: cite.url,
        citationTitle: cite.title,
        query: lastUserMessage.text,
      });
    }
  };

  const handleSuggestionClick = (suggestion: string) => {
    if (lastUserMessage) {
      trackSuggestionClick({
        suggestion,
        originalQuery: lastUserMessage.text,
      });
    }
    sendMessage(suggestion);
  };

  if (!isOpen) return null;

  return (
    <div
      ref={panelRef}
      className={styles.panel}
      style={{ width: panelWidth }}
      data-resizing={isResizing}
    >
      {/* Resize Handle */}
      <div
        className={styles.resizeHandle}
        onMouseDown={handleMouseDown}
      >
        <GripVertical size={12} className={styles.resizeGrip} />
      </div>

      <div className={styles.chatContainer}>
        {/* Header */}
        <header className={styles.header}>
          <span className={styles.title}>Ask AI</span>
          <div className={styles.headerActions}>
            {messages.length > 0 && (
              <button
                onClick={clearMessages}
                className={styles.iconButton}
                title="Clear conversation"
              >
                <Trash2 size={14} />
              </button>
            )}
            <button
              onClick={() => setIsOpen(false)}
              className={styles.iconButton}
              title="Close panel"
            >
              <X size={16} />
            </button>
          </div>
        </header>

        {/* Messages */}
        <main className={styles.messages}>
          {messages.length === 0 ? (
            <div className={styles.welcome}>
              <div className={styles.welcomeIcon}>?</div>
              <h3>Ask about BAML</h3>
              <p>I can help you with syntax, concepts, examples, and more.</p>
              <div className={styles.suggestions}>
                {[
                  'How do I define a function?',
                  'What are clients in BAML?',
                  'How does type validation work?',
                ].map((q, i) => (
                  <button
                    key={i}
                    onClick={() => sendMessage(q)}
                    className={styles.suggestionChip}
                  >
                    {q}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            messages.map(msg => (
              <div
                key={msg.id}
                className={`${styles.message} ${styles[msg.role]} ${msg.isStreaming ? styles.streaming : ''}`}
              >
                {msg.role === 'assistant' ? (
                  <>
                    <ReactMarkdown
                      remarkPlugins={[remarkGfm]}
                      components={markdownComponents}
                    >
                      {msg.text || ''}
                    </ReactMarkdown>
                    {msg.isStreaming && !msg.text && (
                      <div className={styles.loadingDots}>
                        <span />
                        <span />
                        <span />
                      </div>
                    )}
                    {msg.isStreaming && msg.text && (
                      <span className={styles.streamingCursor} />
                    )}
                  </>
                ) : (
                  msg.text
                )}
              </div>
            ))
          )}

          <div ref={messagesEndRef} />
        </main>

        {/* Citations */}
        {lastMessage?.citations && lastMessage.citations.length > 0 && (
          <div className={styles.citations}>
            <span className={styles.citationsLabel}>Sources</span>
            <div className={styles.citationsList}>
              {lastMessage.citations.map((cite, i) => (
                <a
                  key={i}
                  href={cite.url}
                  className={styles.citation}
                  onClick={() => handleCitationClick(cite)}
                >
                  {cite.title}
                </a>
              ))}
            </div>
          </div>
        )}

        {/* Suggested follow-ups */}
        {lastMessage?.suggested_questions && lastMessage.suggested_questions.length > 0 && (
          <div className={styles.followUps}>
            {lastMessage.suggested_questions.map((q, i) => (
              <button
                key={i}
                onClick={() => handleSuggestionClick(q)}
                className={styles.followUpChip}
              >
                {q}
              </button>
            ))}
          </div>
        )}

        {/* Feedback */}
        {lastMessage?.role === 'assistant' && lastUserMessage && (
          <Feedback
            query={lastUserMessage.text}
            answer={lastMessage.text}
          />
        )}

        {/* Input */}
        <form onSubmit={handleSubmit} className={styles.inputForm}>
          <textarea
            ref={inputRef}
            value={input}
            onChange={e => {
              setInput(e.target.value);
            }}
            onKeyDown={handleKeyDown}
            placeholder="Ask a question..."
            disabled={isLoading}
            rows={1}
            className={styles.input}
          />
          {streamingMessageId ? (
            <button
              type="button"
              onClick={stopStreaming}
              className={styles.stopButton}
              title="Stop generating"
            >
              <Square size={14} />
            </button>
          ) : (
            <button
              type="submit"
              disabled={isLoading || !input.trim()}
              className={styles.sendButton}
            >
              <Send size={16} />
            </button>
          )}
        </form>
      </div>
    </div>
  );
}
