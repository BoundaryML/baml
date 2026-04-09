import { useState, useCallback, useEffect, useRef, createContext, useContext, ReactNode } from 'react';
import {
  initAnalytics,
  trackAssistantOpened,
  trackQuery,
  trackResponse,
  trackError,
  trackSessionEnded,
} from '../../../lib/analytics';

interface Message {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  citations?: Array<{ title: string; url: string; relevance: string }>;
  suggested_questions?: string[];
  isStreaming?: boolean;
}

interface ChatState {
  messages: Message[];
  isLoading: boolean;
  isOpen: boolean;
  streamingMessageId: string | null;
}

interface ChatContextValue extends ChatState {
  sendMessage: (text: string) => Promise<void>;
  setIsOpen: (isOpen: boolean, source?: 'button' | 'keyboard') => void;
  clearMessages: () => void;
  sessionId: string;
  stopStreaming: () => void;
}

const ChatContext = createContext<ChatContextValue | null>(null);

export function useChat(): ChatContextValue {
  const context = useContext(ChatContext);
  if (!context) {
    throw new Error('useChat must be used within a ChatProvider');
  }
  return context;
}

interface ChatProviderProps {
  children: ReactNode;
}

export function ChatProvider({ children }: ChatProviderProps) {
  const [state, setState] = useState<ChatState>({
    messages: [],
    isLoading: false,
    isOpen: false,
    streamingMessageId: null,
  });

  const [sessionId] = useState(() => `session-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`);
  const sessionStartTime = useRef<number>(Date.now());
  const abortControllerRef = useRef<AbortController | null>(null);

  // Initialize analytics
  useEffect(() => {
    initAnalytics();
  }, []);

  // Load persisted state from localStorage
  useEffect(() => {
    if (typeof window === 'undefined') return;
    
    // Load messages
    const savedMessages = localStorage.getItem('ask-baml-messages');
    if (savedMessages) {
      try {
        const messages = JSON.parse(savedMessages);
        setState(prev => ({ ...prev, messages }));
      } catch {
        // Ignore parse errors
      }
    }
    
    // Load isOpen state
    const savedIsOpen = localStorage.getItem('ask-baml-panel-open');
    if (savedIsOpen) {
      try {
        const isOpen = JSON.parse(savedIsOpen);
        setState(prev => ({ ...prev, isOpen }));
      } catch {
        // Ignore parse errors
      }
    }
  }, []);

  // Persist messages to localStorage
  useEffect(() => {
    if (typeof window === 'undefined') return;
    localStorage.setItem('ask-baml-messages', JSON.stringify(state.messages));
  }, [state.messages]);

  // Persist isOpen state to localStorage
  useEffect(() => {
    if (typeof window === 'undefined') return;
    localStorage.setItem('ask-baml-panel-open', JSON.stringify(state.isOpen));
  }, [state.isOpen]);

  // Keyboard shortcuts
  useEffect(() => {
    if (typeof window === 'undefined') return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setState(prev => ({ ...prev, isOpen: !prev.isOpen }));
      }
      if (e.key === 'Escape' && state.isOpen) {
        setState(prev => ({ ...prev, isOpen: false }));
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [state.isOpen]);

  const setIsOpen = useCallback((isOpen: boolean, source?: 'button' | 'keyboard') => {
    if (isOpen && !state.isOpen) {
      sessionStartTime.current = Date.now();
      if (source) {
        trackAssistantOpened(source);
      }
    }

    // Track session end when closing
    if (!isOpen && state.isOpen && state.messages.length > 0) {
      trackSessionEnded({
        sessionId,
        messageCount: state.messages.length,
        durationSeconds: Math.round((Date.now() - sessionStartTime.current) / 1000),
      });
    }

    setState(prev => ({ ...prev, isOpen }));
  }, [state.isOpen, state.messages.length, sessionId]);

  const stopStreaming = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  }, []);

  const sendMessage = useCallback(async (text: string) => {
    if (!text.trim() || state.isLoading) return;

    const startTime = Date.now();
    const trimmedText = text.trim();

    // Track query
    trackQuery({
      query: trimmedText,
      sessionId,
      conversationLength: state.messages.length,
    });

    const userMessage: Message = {
      id: `user-${Date.now()}`,
      role: 'user',
      text: trimmedText,
    };

    const assistantMessageId = `assistant-${Date.now()}`;

    // Create assistant message placeholder for streaming
    const assistantMessage: Message = {
      id: assistantMessageId,
      role: 'assistant',
      text: '',
      isStreaming: true,
    };

    setState(prev => ({
      ...prev,
      messages: [...prev.messages, userMessage, assistantMessage],
      isLoading: true,
      streamingMessageId: assistantMessageId,
    }));

    // Create abort controller for this request
    abortControllerRef.current = new AbortController();

    try {
      const response = await fetch('/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: trimmedText,
          prev_messages: state.messages.map(m => ({ role: m.role, text: m.text })),
          stream: true,
        }),
        signal: abortControllerRef.current.signal,
      });

      if (!response.ok) throw new Error('Request failed');

      const reader = response.body?.getReader();
      if (!reader) throw new Error('No response body');

      const decoder = new TextDecoder();
      let buffer = '';
      let docScores: number[] = [];
      let finalData: { answer: string; citations: any[]; suggested_questions: string[] } | null = null;

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || ''; // Keep incomplete line in buffer

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            try {
              const data = JSON.parse(line.slice(6));

              if (data.type === 'doc_scores') {
                docScores = data.scores;
              } else if (data.type === 'partial') {
                // Update streaming message with partial content
                setState(prev => ({
                  ...prev,
                  messages: prev.messages.map(m =>
                    m.id === assistantMessageId
                      ? {
                          ...m,
                          text: data.answer || '',
                          citations: data.citations,
                          suggested_questions: data.suggested_questions,
                        }
                      : m
                  ),
                }));
              } else if (data.type === 'done') {
                finalData = data;
              }
            } catch (e) {
              console.warn('Failed to parse SSE data:', e);
            }
          }
        }
      }

      // Finalize the message
      if (finalData) {
        // Track successful response
        trackResponse({
          query: trimmedText,
          answer: finalData.answer,
          citations: finalData.citations || [],
          suggestedQuestions: finalData.suggested_questions || [],
          latencyMs: Date.now() - startTime,
          topDocScores: docScores,
        });

        setState(prev => ({
          ...prev,
          messages: prev.messages.map(m =>
            m.id === assistantMessageId
              ? {
                  ...m,
                  text: finalData!.answer,
                  citations: finalData!.citations,
                  suggested_questions: finalData!.suggested_questions,
                  isStreaming: false,
                }
              : m
          ),
          isLoading: false,
          streamingMessageId: null,
        }));
      } else {
        // No final data received, mark as complete anyway
        setState(prev => ({
          ...prev,
          messages: prev.messages.map(m =>
            m.id === assistantMessageId ? { ...m, isStreaming: false } : m
          ),
          isLoading: false,
          streamingMessageId: null,
        }));
      }
    } catch (error) {
      // Handle abort
      if (error instanceof Error && error.name === 'AbortError') {
        setState(prev => ({
          ...prev,
          messages: prev.messages.map(m =>
            m.id === assistantMessageId
              ? { ...m, isStreaming: false, text: m.text || 'Response stopped.' }
              : m
          ),
          isLoading: false,
          streamingMessageId: null,
        }));
        return;
      }

      console.error('Chat error:', error);

      // Track error
      trackError({
        query: trimmedText,
        errorType: error instanceof Error ? error.name : 'UnknownError',
        errorMessage: error instanceof Error ? error.message : 'Unknown error',
      });

      setState(prev => ({
        ...prev,
        messages: prev.messages.map(m =>
          m.id === assistantMessageId
            ? { ...m, text: 'Sorry, something went wrong. Please try again.', isStreaming: false }
            : m
        ),
        isLoading: false,
        streamingMessageId: null,
      }));
    } finally {
      abortControllerRef.current = null;
    }
  }, [state.messages, state.isLoading, sessionId]);

  const clearMessages = useCallback(() => {
    setState(prev => ({ ...prev, messages: [] }));
  }, []);

  const value: ChatContextValue = {
    ...state,
    sendMessage,
    setIsOpen,
    clearMessages,
    sessionId,
    stopStreaming,
  };

  return (
    <ChatContext.Provider value={value}>
      {children}
    </ChatContext.Provider>
  );
}
