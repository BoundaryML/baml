import { type Message, type QueryRequest, QueryResponseSchema } from '@baml/sage-interface';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import React, { useRef, useEffect, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import BamlLambWhite from './baml-lamb-white.svg';
import { AssistantResponseFeedback } from './lib/AssistantResponseFeedback';
import {
  type StoredMessage,
  messagesAtom,
  pendingQueryAtom,
  resetSessionAtom,
  sessionIdAtom,
} from './store';

const OPEN_BY_DEFAULT = true;
const SESSION_STORAGE_KEY = 'baml-ai-context';

interface ChatBotProps {
  apiEndpoint?: string;
  isOpen?: boolean;
  onClose?: () => void;
}

// Transform messages for API format
const transformMessagesForAPI = (messages: StoredMessage[]): Array<Message> => {
  return messages
    .filter((msg) => msg.role === 'user' || msg.role === 'assistant')
    .map((msg) => {
      switch (msg.role) {
        case 'user':
          return msg;
        case 'assistant':
          return msg;
        default:
          throw new Error('Unexpected message type in transform');
      }
    });
};

// Serialize errors to storable format
const serializeError = (
  error: unknown,
): { message: string; code?: string; statusCode?: number } => {
  if (error instanceof Error) {
    const serialized: { message: string; code?: string; statusCode?: number } = {
      message: error.message,
    };
    if ('code' in error && error.code) serialized.code = String(error.code);
    if ('statusCode' in error && error.statusCode) serialized.statusCode = Number(error.statusCode);

    // Handle specific HTTP errors
    if (error.message.includes('HTTP error! status:')) {
      const match = error.message.match(/status: (\d+)/);
      if (match) {
        serialized.statusCode = Number.parseInt(match[1]!);
      }
    }

    return serialized;
  }
  return { message: String(error) };
};

const postDocChat = async (req: QueryRequest) => {
  const response = await fetch(API_ENDPOINT, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(req),
  });

  if (!response.ok) {
    throw new Error(`HTTP error! status: ${response.status}`);
  }

  const data = await response.json();
  return QueryResponseSchema.parse(data);
};

const API_ENDPOINT =
  process.env.NODE_ENV === 'development'
    ? 'http://localhost:4000/api/doc-chat'
    : 'https://boundary-sage-backend.vercel.app/api/ask-baml-chat';

const ChatBot: React.FC<ChatBotProps> = ({ isOpen = OPEN_BY_DEFAULT, onClose }) => {
  const [messages, setMessages] = useAtom(messagesAtom);
  const sessionId = useAtomValue(sessionIdAtom);
  const resetSession = useSetAtom(resetSessionAtom);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [pendingQuery, setPendingQuery] = useAtom(pendingQueryAtom);
  // Add width state for resizing
  const [width, setWidth] = useState(400);
  const [isResizing, setIsResizing] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  // Clear chat functionality - now resets session ID too
  const clearChat = () => {
    resetSession();
  };

  const sendMessage = async (text: string) => {
    if (!text.trim()) return;

    // Add user message
    const userMessage: StoredMessage = {
      id: Date.now().toString(),
      role: 'user',
      text: text.trim(),
      timestamp: new Date(),
    };

    // Add progress message
    const progressMessage: StoredMessage = {
      id: (Date.now() + 1).toString(),
      role: 'assistant/progress',
      timestamp: new Date(),
    };

    const messagesWithProgress = [...messages, userMessage, progressMessage];
    setMessages(messagesWithProgress);
    setInput('');

    setIsLoading(true);

    try {
      const data = await postDocChat({
        session_id: sessionId,
        message: {
          role: 'user',
          text: text.trim(),
        },
        // TODO: add language preference
        prev_messages: transformMessagesForAPI(messagesWithProgress),
      });

      // Create success message with the response
      const successMessage: StoredMessage = {
        id: progressMessage.id,
        timestamp: new Date(),
        ...data.message,
      };

      // Replace progress message with success message
      setMessages(
        messagesWithProgress.map((msg) => (msg.id === progressMessage.id ? successMessage : msg)),
      );

      // Auto-navigate to first very-relevant doc on same domain if available
      if (data.message.ranked_docs && data.message.ranked_docs.length > 0) {
        const veryRelevantSameDomainDoc = data.message.ranked_docs.find(
          (doc) => doc.relevance === 'very-relevant' && doc.url.startsWith('/'),
        );
        if (veryRelevantSameDomainDoc && (window as any).navigateToDoc) {
          (window as any).navigateToDoc(
            {
              u: veryRelevantSameDomainDoc.url,
              t: veryRelevantSameDomainDoc.title,
              sel: 'article',
            },
            text.trim(),
          );
        }
      }
    } catch (error) {
      console.error('Error sending message:', error);

      // Create error message
      const errorMessage: StoredMessage = {
        id: progressMessage.id,
        role: 'assistant/error',
        timestamp: new Date(),
        error: serializeError(error),
      };

      // Replace progress message with error message
      setMessages(
        messagesWithProgress.map((msg) => (msg.id === progressMessage.id ? errorMessage : msg)),
      );
    } finally {
      setIsLoading(false);
    }
  };

  // Handle pending query from Ask Baaaaml functionality
  useEffect(() => {
    if (pendingQuery && !isLoading) {
      sendMessage(pendingQuery);
      setPendingQuery(null);
    }
  }, [pendingQuery, isLoading, setPendingQuery]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    sendMessage(input);
  };

  const handleRetry = () => {
    // Get the last user message to retry
    const lastUserMessage = [...messages].reverse().find((msg) => msg.role === 'user');
    if (lastUserMessage && lastUserMessage.role === 'user') {
      sendMessage(lastUserMessage.text);
    }
  };

  const handleClose = () => {
    if (onClose) {
      onClose();
    }
  };

  // Add resize handlers
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsResizing(true);
  };

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizing) return;

      const newWidth = window.innerWidth - e.clientX;
      // Set min and max width constraints
      const minWidth = 300;
      const maxWidth = Math.min(800, window.innerWidth * 0.8);

      if (newWidth >= minWidth && newWidth <= maxWidth) {
        setWidth(newWidth);
      }
    };

    const handleMouseUp = () => {
      setIsResizing(false);
    };

    if (isResizing) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = 'ew-resize';
      document.body.style.userSelect = 'none';
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  // Calculate panel position below header
  const [panelTop, setPanelTop] = React.useState(0);
  const [panelHeight, setPanelHeight] = React.useState('100vh');

  React.useEffect(() => {
    const updatePosition = () => {
      const header = document.querySelector('header, .fern-header') as HTMLElement;
      const top = header ? header.getBoundingClientRect().bottom : 0;
      setPanelTop(top);
      setPanelHeight(`calc(100vh - ${top}px)`);
    };

    updatePosition();
    window.addEventListener('resize', updatePosition);
    window.addEventListener('scroll', updatePosition, { passive: true });

    return () => {
      window.removeEventListener('resize', updatePosition);
      window.removeEventListener('scroll', updatePosition);
    };
  }, []);

  console.log('messages', messages);

  return (
    <div
      style={{
        position: 'fixed',
        top: `${panelTop}px`,
        right: '0',
        width: `${width}px`,
        height: panelHeight,
        backgroundColor: '#ffffff',
        borderLeft: '1px solid #e5e7eb',
        transform: isOpen ? 'translateX(0)' : 'translateX(100%)',
        transition: isResizing ? 'none' : 'transform .3s cubic-bezier(.4,0,.2,1)',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 2000,
        fontFamily: 'Inter, system-ui, -apple-system, BlinkMacSystemFont, sans-serif',
        overflow: 'hidden',
      }}
    >
      {/* Resize Handle */}
      <div
        onMouseDown={handleMouseDown}
        style={{
          position: 'absolute',
          left: '0',
          top: '0',
          bottom: '0',
          width: '4px',
          cursor: 'ew-resize',
          backgroundColor: 'transparent',
          zIndex: 10,
          transition: 'background-color 0.2s ease',
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.backgroundColor = '#7d47e3';
        }}
        onMouseLeave={(e) => {
          if (!isResizing) {
            e.currentTarget.style.backgroundColor = 'transparent';
          }
        }}
      />

      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          height: '64px',
          padding: '0 24px',
          fontSize: '16px',
          fontWeight: '700',
          backgroundColor: '#ffffff',
          color: '#111827',
          borderBottom: '1px solid #e5e7eb',
          position: 'relative',
          zIndex: 1,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {/* BAML Icon */}
          <div
            style={{
              width: '28px',
              height: '28px',
              borderRadius: '6px',
              background: '#7d47e3',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              padding: '2px',
            }}
          >
            <img
              src={BamlLambWhite}
              alt="BAML Logo"
              style={{
                width: '26px',
                height: '26px',
                filter: 'none',
              }}
            />
          </div>
          <span style={{ color: '#111827' }}>Ask Baaaml</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {/* Clear Chat Button */}
          {messages.length > 0 && (
            <button
              type="button"
              onClick={clearChat}
              style={{
                background: 'none',
                border: '1px solid #e5e7eb',
                fontSize: '12px',
                color: '#6b7280',
                cursor: 'pointer',
                padding: '6px 12px',
                borderRadius: '6px',
                fontWeight: '500',
                transition: 'all 0.2s ease',
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.backgroundColor = '#f9fafb';
                e.currentTarget.style.borderColor = '#d1d5db';
                e.currentTarget.style.color = '#374151';
              }}
              onFocus={(e) => {
                e.currentTarget.style.backgroundColor = '#f9fafb';
                e.currentTarget.style.borderColor = '#d1d5db';
                e.currentTarget.style.color = '#374151';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.backgroundColor = 'transparent';
                e.currentTarget.style.borderColor = '#e5e7eb';
                e.currentTarget.style.color = '#6b7280';
              }}
              onBlur={(e) => {
                e.currentTarget.style.backgroundColor = 'transparent';
                e.currentTarget.style.borderColor = '#e5e7eb';
                e.currentTarget.style.color = '#6b7280';
              }}
            >
              Clear
            </button>
          )}
          {/* Close Button */}
          <button
            type="button"
            onClick={handleClose}
            style={{
              background: 'none',
              border: 'none',
              fontSize: '20px',
              color: '#6b7280',
              cursor: 'pointer',
              opacity: 0.7,
              padding: '4px',
              lineHeight: 1,
              transition: 'all 0.2s ease',
              borderRadius: '4px',
            }}
            onMouseOver={(e) => {
              e.currentTarget.style.opacity = '1';
              e.currentTarget.style.backgroundColor = '#f3f4f6';
            }}
            onFocus={(e) => {
              e.currentTarget.style.opacity = '1';
              e.currentTarget.style.backgroundColor = '#f3f4f6';
            }}
            onMouseOut={(e) => {
              e.currentTarget.style.opacity = '0.7';
              e.currentTarget.style.backgroundColor = 'transparent';
            }}
            onBlur={(e) => {
              e.currentTarget.style.opacity = '0.7';
              e.currentTarget.style.backgroundColor = 'transparent';
            }}
          >
            ✕
          </button>
        </div>
      </div>

      {/* Messages */}
      <main
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: '24px',
          display: 'flex',
          flexDirection: 'column',
          backgroundColor: '#fafafa',
          gap: '16px',
        }}
        className="chat-scrollbar"
      >
        {messages.length === 0 && (
          <div
            style={{
              textAlign: 'center',
              color: '#6b7280',
              fontStyle: 'normal',
              marginTop: '40px',
              padding: '24px',
              backgroundColor: '#ffffff',
              borderRadius: '12px',
              border: '1px solid #e5e7eb',
            }}
          >
            <div style={{ fontSize: '24px', marginBottom: '8px' }}>👋</div>
            <div
              style={{
                fontWeight: '600',
                marginBottom: '4px',
                color: '#111827',
              }}
            >
              Welcome to BAML Assistant
            </div>
            <div style={{ fontSize: '14px', lineHeight: '1.5' }}>
              I'm here to help you with the BAML documentation. Ask me anything about functions,
              types, clients, or examples!
            </div>
          </div>
        )}

        {messages.map((message) => {
          if (message.role === 'assistant/progress') {
            return (
              <div
                key={message.id}
                style={{
                  maxWidth: '85%',
                  padding: '12px 16px',
                  borderRadius: '16px 16px 16px 4px',
                  fontSize: '14px',
                  lineHeight: '1.5',
                  alignSelf: 'flex-start',
                  backgroundColor: '#ffffff',
                  color: '#6b7280',
                  border: '1px solid #e5e7eb',
                  boxShadow: 'none',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                }}
              >
                <div
                  style={{
                    width: '6px',
                    height: '6px',
                    borderRadius: '50%',
                    backgroundColor: '#7d47e3',
                    animation: 'pulse 1.5s ease-in-out infinite',
                  }}
                />
                <div
                  style={{
                    width: '6px',
                    height: '6px',
                    borderRadius: '50%',
                    backgroundColor: '#7d47e3',
                    animation: 'pulse 1.5s ease-in-out infinite 0.2s',
                  }}
                />
                <div
                  style={{
                    width: '6px',
                    height: '6px',
                    borderRadius: '50%',
                    backgroundColor: '#7d47e3',
                    animation: 'pulse 1.5s ease-in-out infinite 0.4s',
                  }}
                />
                <span>Thinking...</span>
              </div>
            );
          }

          return (
            <div
              key={message.id}
              style={{
                alignSelf: message.role === 'user' ? 'flex-end' : 'flex-start',
                maxWidth: '85%',
                display: 'flex',
                flexDirection: 'column',
                gap: '8px',
              }}
            >
              <div
                style={{
                  padding: '12px 16px',
                  borderRadius:
                    message.role === 'user' ? '16px 16px 4px 16px' : '16px 16px 16px 4px',
                  fontSize: '14px',
                  lineHeight: '1.5',
                  backgroundColor:
                    message.role === 'user'
                      ? '#7d47e3'
                      : message.role === 'assistant/error'
                        ? '#fef2f2'
                        : '#ffffff',
                  color:
                    message.role === 'user'
                      ? '#ffffff'
                      : message.role === 'assistant/error'
                        ? '#dc2626'
                        : '#111827',
                  wordWrap: 'break-word',
                  border: message.role === 'user' ? 'none' : '1px solid #e5e7eb',
                  boxShadow: 'none',
                }}
              >
                {message.role === 'user' ? (
                  message.text
                ) : message.role === 'assistant/error' ? (
                  <div>
                    {(() => {
                      let errorText = 'Sorry, there was an error processing your request.';
                      const errorMsg = message.error.message;
                      const statusCode = message.error.statusCode;

                      if (errorMsg.includes('fetch')) {
                        errorText =
                          'Unable to connect to the AI service. Please check your connection and try again.';
                      } else if (statusCode === 429) {
                        errorText = 'Too many requests. Please wait a moment and try again.';
                      } else if (statusCode === 500) {
                        errorText =
                          'Server error occurred. The AI service may be temporarily unavailable.';
                      } else if (statusCode === 404) {
                        errorText =
                          'AI service endpoint not found. Please check the configuration.';
                      } else if (errorMsg.includes('HTTP error!')) {
                        errorText = `Service error: ${errorMsg}. Please try again.`;
                      } else if (errorMsg.includes('JSON')) {
                        errorText = 'Received invalid response from AI service. Please try again.';
                      }

                      return errorText;
                    })()}
                  </div>
                ) : message.role === 'assistant' ? (
                  <ReactMarkdown
                    remarkPlugins={[remarkGfm]}
                    components={{
                      // Custom styles for markdown elements
                      p: ({ children }) => (
                        <p style={{ margin: '0 0 12px 0', lineHeight: '1.6' }}>{children}</p>
                      ),
                      h1: ({ children }) => (
                        <h1
                          style={{
                            fontSize: '18px',
                            fontWeight: '700',
                            margin: '0 0 12px 0',
                            color: '#111827',
                          }}
                        >
                          {children}
                        </h1>
                      ),
                      h2: ({ children }) => (
                        <h2
                          style={{
                            fontSize: '16px',
                            fontWeight: '600',
                            margin: '0 0 10px 0',
                            color: '#111827',
                          }}
                        >
                          {children}
                        </h2>
                      ),
                      h3: ({ children }) => (
                        <h3
                          style={{
                            fontSize: '15px',
                            fontWeight: '600',
                            margin: '0 0 8px 0',
                            color: '#111827',
                          }}
                        >
                          {children}
                        </h3>
                      ),
                      code: ({ children, className, ...props }) => {
                        const isInline = !className;

                        return isInline ? (
                          <code
                            {...props}
                            style={{
                              backgroundColor: '#f3f4f6',
                              padding: '2px 6px',
                              borderRadius: '4px',
                              fontSize: '13px',
                              fontFamily:
                                'Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
                              color: '#d63384',
                            }}
                          >
                            {children}
                          </code>
                        ) : (
                          <pre
                            style={{
                              margin: '0 0 12px 0',
                              borderRadius: '6px',
                              fontSize: '13px',
                              border: '1px solid #e9ecef',
                              backgroundColor: '#f8f9fa',
                              padding: '12px',
                              overflow: 'auto',
                              fontFamily:
                                'Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
                            }}
                          >
                            <code style={{ backgroundColor: 'transparent' }}>
                              {String(children).replace(/\n$/, '')}
                            </code>
                          </pre>
                        );
                      },
                      pre: ({ children }) => (
                        <pre
                          style={{
                            margin: '0 0 12px 0',
                            overflow: 'visible',
                          }}
                        >
                          {children}
                        </pre>
                      ),
                      ul: ({ children }) => (
                        <ul
                          style={{
                            margin: '0 0 12px 0',
                            paddingLeft: '20px',
                            listStyleType: 'disc',
                          }}
                        >
                          {children}
                        </ul>
                      ),
                      ol: ({ children }) => (
                        <ol
                          style={{
                            margin: '0 0 12px 0',
                            paddingLeft: '20px',
                            listStyleType: 'decimal',
                          }}
                        >
                          {children}
                        </ol>
                      ),
                      li: ({ children }) => (
                        <li
                          style={{
                            margin: '0 0 4px 0',
                            lineHeight: '1.5',
                          }}
                        >
                          {children}
                        </li>
                      ),
                      blockquote: ({ children }) => (
                        <blockquote
                          style={{
                            margin: '0 0 12px 0',
                            paddingLeft: '16px',
                            borderLeft: '4px solid #7d47e3',
                            fontStyle: 'italic',
                            color: '#6b7280',
                          }}
                        >
                          {children}
                        </blockquote>
                      ),
                      a: ({ children, href }) => (
                        <a
                          href={href}
                          target="_blank"
                          rel="noopener noreferrer"
                          style={{
                            color: '#7d47e3',
                            textDecoration: 'underline',
                            fontWeight: '500',
                          }}
                        >
                          {children}
                        </a>
                      ),
                      table: ({ children }) => (
                        <table
                          style={{
                            width: '100%',
                            borderCollapse: 'collapse',
                            margin: '0 0 12px 0',
                            fontSize: '13px',
                          }}
                        >
                          {children}
                        </table>
                      ),
                      th: ({ children }) => (
                        <th
                          style={{
                            border: '1px solid #e5e7eb',
                            padding: '8px 12px',
                            backgroundColor: '#f9fafb',
                            fontWeight: '600',
                            textAlign: 'left',
                          }}
                        >
                          {children}
                        </th>
                      ),
                      td: ({ children }) => (
                        <td
                          style={{
                            border: '1px solid #e5e7eb',
                            padding: '8px 12px',
                          }}
                        >
                          {children}
                        </td>
                      ),
                    }}
                  >
                    {message.text || "Sorry, I'm not sure how to answer that."}
                  </ReactMarkdown>
                ) : null}

                {/* Feedback buttons for assistant messages */}
                {message.role === 'assistant' && (
                  <AssistantResponseFeedback messageId={message.message_id} />
                )}

                {message.role === 'assistant/error' && (
                  <div style={{ marginTop: '12px' }}>
                    <button
                      type="button"
                      onClick={() => handleRetry()}
                      style={{
                        padding: '8px 16px',
                        fontSize: '13px',
                        backgroundColor: '#6366f1',
                        color: '#fff',
                        border: 'none',
                        borderRadius: '8px',
                        cursor: 'pointer',
                        fontWeight: '500',
                        transition: 'all 0.2s ease',
                        display: 'flex',
                        alignItems: 'center',
                        gap: '6px',
                      }}
                      onMouseOver={(e) => {
                        e.currentTarget.style.backgroundColor = '#5d68e4';
                        e.currentTarget.style.transform = 'translateY(-1px)';
                      }}
                      onFocus={(e) => {
                        e.currentTarget.style.backgroundColor = '#5d68e4';
                        e.currentTarget.style.transform = 'translateY(-1px)';
                      }}
                      onMouseOut={(e) => {
                        e.currentTarget.style.backgroundColor = '#6366f1';
                        e.currentTarget.style.transform = 'translateY(0)';
                      }}
                      onBlur={(e) => {
                        e.currentTarget.style.backgroundColor = '#6366f1';
                        e.currentTarget.style.transform = 'translateY(0)';
                      }}
                    >
                      <svg
                        width="14"
                        height="14"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                        aria-label="Retry"
                      >
                        <title>Retry Icon</title>
                        <path
                          strokeLinecap="round"
                          strokeLinejoin="round"
                          strokeWidth={2}
                          d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                        />
                      </svg>
                      Try Again
                    </button>
                  </div>
                )}
              </div>

              {/* Related docs */}
              {message.role === 'assistant' &&
                message.ranked_docs &&
                message.ranked_docs.length > 0 && (
                  <div
                    style={{
                      fontSize: '12px',
                      color: '#6b7280',
                      padding: '12px',
                      backgroundColor: '#ffffff',
                      borderRadius: '8px',
                      border: '1px solid #e5e7eb',
                    }}
                  >
                    <div
                      style={{
                        fontWeight: '600',
                        marginBottom: '8px',
                        color: '#374151',
                      }}
                    >
                      📖 Related documentation:
                    </div>
                    {message.ranked_docs.map((doc) => (
                      <div key={doc.url} style={{ marginBottom: '4px' }}>
                        <a
                          href={doc.url}
                          onClick={(e) => {
                            // Only redirect if very-relevant and same domain, otherwise use normal link behavior
                            if (doc.relevance === 'very-relevant' && doc.url.startsWith('/')) {
                              e.preventDefault();
                              // Use the global navigateToDoc function to navigate while keeping chat open
                              if ((window as any).navigateToDoc) {
                                (window as any).navigateToDoc(
                                  { u: doc.url, t: doc.title, sel: 'article' },
                                  message.text || '',
                                );
                              } else {
                                // Fallback to normal navigation if navigateToDoc is not available
                                window.location.href = doc.url;
                              }
                            }
                            // For non-very-relevant docs or external links, let the default link behavior handle it
                          }}
                          style={{
                            color: '#7d47e3',
                            textDecoration: 'none',
                            fontSize: '12px',
                            transition: 'all 0.2s ease',
                            fontWeight: '500',
                            cursor: 'pointer',
                          }}
                          onMouseOver={(e) => {
                            e.currentTarget.style.textDecoration = 'underline';
                            e.currentTarget.style.color = '#6b3bc9';
                          }}
                          onFocus={(e) => {
                            e.currentTarget.style.textDecoration = 'underline';
                            e.currentTarget.style.color = '#6b3bc9';
                          }}
                          onMouseOut={(e) => {
                            e.currentTarget.style.textDecoration = 'none';
                            e.currentTarget.style.color = '#7d47e3';
                          }}
                          onBlur={(e) => {
                            e.currentTarget.style.textDecoration = 'none';
                            e.currentTarget.style.color = '#7d47e3';
                          }}
                        >
                          {doc.title}
                        </a>
                      </div>
                    ))}
                  </div>
                )}

              {/* Suggestions */}
              {message.role === 'assistant' &&
                message.suggested_messages &&
                message.suggested_messages.length > 0 && (
                  <div
                    style={{
                      fontSize: '12px',
                      color: '#6b7280',
                      padding: '12px',
                      backgroundColor: '#ffffff',
                      borderRadius: '8px',
                      border: '1px solid #e5e7eb',
                    }}
                  >
                    <div
                      style={{
                        fontWeight: '600',
                        marginBottom: '8px',
                        color: '#374151',
                      }}
                    >
                      💡 Suggested follow-ups:
                    </div>
                    {message.suggested_messages.map((suggestion, index) => (
                      <div key={index} style={{ marginBottom: '4px' }}>
                        <button
                          onClick={() => sendMessage(suggestion)}
                          style={{
                            background: 'none',
                            border: 'none',
                            color: '#7d47e3',
                            textDecoration: 'none',
                            fontSize: '12px',
                            transition: 'all 0.2s ease',
                            fontWeight: '500',
                            cursor: 'pointer',
                            padding: '4px 8px',
                            textAlign: 'left',
                            width: '100%',
                            borderRadius: '4px',
                          }}
                          onMouseOver={(e) => {
                            e.currentTarget.style.backgroundColor = '#f3f4f6';
                            e.currentTarget.style.textDecoration = 'underline';
                            e.currentTarget.style.color = '#6b3bc9';
                          }}
                          onFocus={(e) => {
                            e.currentTarget.style.backgroundColor = '#f3f4f6';
                            e.currentTarget.style.textDecoration = 'underline';
                            e.currentTarget.style.color = '#6b3bc9';
                          }}
                          onMouseOut={(e) => {
                            e.currentTarget.style.backgroundColor = 'transparent';
                            e.currentTarget.style.textDecoration = 'none';
                            e.currentTarget.style.color = '#7d47e3';
                          }}
                          onBlur={(e) => {
                            e.currentTarget.style.backgroundColor = 'transparent';
                            e.currentTarget.style.textDecoration = 'none';
                            e.currentTarget.style.color = '#7d47e3';
                          }}
                        >
                          {suggestion}
                        </button>
                      </div>
                    ))}
                  </div>
                )}
            </div>
          );
        })}

        <div ref={messagesEndRef} />
      </main>

      {/* Input Form */}
      <form
        onSubmit={handleSubmit}
        style={{
          display: 'flex',
          padding: '16px 24px',
          borderTop: '1px solid #e5e7eb',
          backgroundColor: '#ffffff',
          gap: '12px',
          alignItems: 'flex-end',
        }}
      >
        <div style={{ flex: 1, position: 'relative' }}>
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                sendMessage(input);
              }
            }}
            placeholder="Ask me anything about BAML..."
            disabled={isLoading}
            rows={1}
            style={{
              width: '100%',
              minHeight: '44px',
              maxHeight: '120px',
              padding: '12px 16px',
              border: '1px solid #e5e7eb',
              borderRadius: '12px',
              fontSize: '14px',
              outline: 'none',
              fontFamily: 'inherit',
              backgroundColor: '#ffffff',
              color: '#111827',
              resize: 'none',
              transition: 'border-color 0.2s ease',
            }}
            onFocus={(e) => {
              e.currentTarget.style.borderColor = '#7d47e3';
            }}
            onBlur={(e) => {
              e.currentTarget.style.borderColor = '#e5e7eb';
            }}
          />
        </div>
        <button
          type="submit"
          disabled={isLoading || !input.trim()}
          style={{
            border: 'none',
            padding: '12px 20px',
            background: input.trim() ? '#7d47e3' : '#e5e7eb',
            color: input.trim() ? '#ffffff' : '#9ca3af',
            fontWeight: '600',
            cursor: isLoading || !input.trim() ? 'not-allowed' : 'pointer',
            borderRadius: '12px',
            fontSize: '14px',
            transition: 'all 0.2s ease',
            minWidth: '64px',
          }}
          onMouseOver={(e) => {
            if (input.trim() && !isLoading) {
              e.currentTarget.style.backgroundColor = '#6b3bc9';
            }
          }}
          onFocus={(e) => {
            if (input.trim() && !isLoading) {
              e.currentTarget.style.backgroundColor = '#6b3bc9';
            }
          }}
          onMouseOut={(e) => {
            if (input.trim() && !isLoading) {
              e.currentTarget.style.backgroundColor = '#7d47e3';
            }
          }}
          onBlur={(e) => {
            if (input.trim() && !isLoading) {
              e.currentTarget.style.backgroundColor = '#7d47e3';
            }
          }}
        >
          Send
        </button>
      </form>

      <style>
        {`
          @keyframes pulse {
            0%, 100% { opacity: 0.4; transform: scale(0.8); }
            50% { opacity: 1; transform: scale(1); }
          }
          
          /* Improved scrollbar styling for chat */
          .chat-scrollbar::-webkit-scrollbar {
            width: 6px;
          }
          .chat-scrollbar::-webkit-scrollbar-track {
            background: transparent;
          }
          .chat-scrollbar::-webkit-scrollbar-thumb {
            background: #d1d5db;
            border-radius: 3px;
          }
          .chat-scrollbar::-webkit-scrollbar-thumb:hover {
            background: #9ca3af;
          }
        `}
      </style>
    </div>
  );
};

export default ChatBot;
