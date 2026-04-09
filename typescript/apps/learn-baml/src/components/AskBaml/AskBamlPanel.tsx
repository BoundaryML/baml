import React, { useRef, useEffect, useState, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import { useChat } from './hooks/useChat';
import { trackCitationClick, trackSuggestionClick } from '../../lib/analytics';
import { Feedback } from './Feedback';
import styles from './styles.module.css';

export function AskBamlPanel() {
  const { messages, isLoading, isOpen, setIsOpen, sendMessage, clearMessages } = useChat();
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  // Focus input when panel opens
  useEffect(() => {
    if (isOpen) inputRef.current?.focus();
  }, [isOpen]);

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

  const handleCitationClick = useCallback((cite: { title: string; url: string }) => {
    if (lastUserMessage) {
      trackCitationClick({
        citationUrl: cite.url,
        citationTitle: cite.title,
        query: lastUserMessage.text,
      });
    }
  }, [lastUserMessage]);

  const handleSuggestionClick = useCallback((suggestion: string) => {
    if (lastUserMessage) {
      trackSuggestionClick({
        suggestion,
        originalQuery: lastUserMessage.text,
      });
    }
    sendMessage(suggestion);
  }, [lastUserMessage, sendMessage]);

  if (!isOpen) return null;

  const lastMessage = messages[messages.length - 1];

  return (
    <div className={styles.panel}>
      <header className={styles.header}>
        <span className={styles.title}>Ask BAML</span>
        <div className={styles.headerActions}>
          {messages.length > 0 && (
            <button onClick={clearMessages} className={styles.clearBtn}>
              Clear
            </button>
          )}
          <button onClick={() => setIsOpen(false)} className={styles.closeBtn}>
            x
          </button>
        </div>
      </header>

      <main className={styles.messages}>
        {messages.length === 0 ? (
          <div className={styles.welcome}>
            <h3>Hi there!</h3>
            <p>Ask me anything about BAML.</p>
            <p className={styles.hint}>Try: "How do I define a function?"</p>
          </div>
        ) : (
          messages.map(msg => (
            <div
              key={msg.id}
              className={`${styles.message} ${styles[msg.role]}`}
            >
              {msg.role === 'assistant' ? (
                <ReactMarkdown>{msg.text}</ReactMarkdown>
              ) : (
                msg.text
              )}
            </div>
          ))
        )}

        {isLoading && (
          <div className={`${styles.message} ${styles.assistant}`}>
            <span className={styles.loading}>Thinking...</span>
          </div>
        )}

        <div ref={messagesEndRef} />
      </main>

      {/* Citations */}
      {lastMessage?.citations && lastMessage.citations.length > 0 && (
        <div className={styles.citations}>
          <span className={styles.citationsLabel}>Sources:</span>
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
      )}

      {/* Suggestions */}
      {lastMessage?.suggested_questions && lastMessage.suggested_questions.length > 0 && (
        <div className={styles.suggestions}>
          {lastMessage.suggested_questions.map((q, i) => (
            <button
              key={i}
              onClick={() => handleSuggestionClick(q)}
              className={styles.suggestion}
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

      <form onSubmit={handleSubmit} className={styles.inputForm}>
        <textarea
          ref={inputRef}
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Ask about BAML..."
          disabled={isLoading}
          rows={1}
          className={styles.input}
        />
        <button
          type="submit"
          disabled={isLoading || !input.trim()}
          className={styles.sendBtn}
        >
          Send
        </button>
      </form>
    </div>
  );
}
