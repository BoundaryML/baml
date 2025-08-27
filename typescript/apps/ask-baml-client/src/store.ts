import type { AssistantMessage, UserMessage } from '@baml/sage-interface';
import { atom, useAtom, useSetAtom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';

const CHATBOT_OPEN_STORAGE_KEY = 'baml-chatbot-open';
const SESSION_STORAGE_KEY = 'baml-chat-session';

export type StoredMessage = {
  id: string;
  timestamp: Date;
} & (
  | UserMessage
  | AssistantMessage
  | {
      role: 'assistant/error';
      error: {
        message: string;
        code?: string;
        statusCode?: number;
      };
    }
  | {
      role: 'assistant/progress';
    }
);

interface SessionData {
  sessionId: string;
  messages: StoredMessage[];
  createdAt: string;
}

const generateSessionId = (): string => {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, '0');
  const day = String(now.getDate()).padStart(2, '0');
  const hour = String(now.getHours()).padStart(2, '0');
  const minute = String(now.getMinutes()).padStart(2, '0');

  // Get timezone abbreviation
  const timezone =
    Intl.DateTimeFormat('en', {
      timeZoneName: 'short',
    })
      .formatToParts(now)
      .find((part) => part.type === 'timeZoneName')?.value || 'UTC';

  const randomValue = Math.random().toString(36).substring(2, 8);

  return `sess-${year}-${month}-${day}-${hour}${minute}-${timezone}-${randomValue}`;
};

const getSessionData = (): SessionData => {
  try {
    const stored = sessionStorage.getItem(SESSION_STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      // Validate structure and that sessionId starts with 'sess-'
      if (
        parsed.sessionId?.startsWith('sess-') &&
        Array.isArray(parsed.messages) &&
        parsed.createdAt
      ) {
        return {
          ...parsed,
          messages: parsed.messages.map((msg: any) => ({
            ...msg,
            timestamp: new Date(msg.timestamp),
          })),
        };
      }
    }
  } catch (error) {
    console.warn('Invalid session data found, creating fresh session:', error);
  }

  // Wipe any invalid/legacy data and create fresh session
  try {
    sessionStorage.removeItem(SESSION_STORAGE_KEY);
    sessionStorage.removeItem('baml-chat-messages'); // Clean up old key if exists
  } catch (e) {
    // Ignore cleanup errors
  }

  return {
    sessionId: generateSessionId(),
    messages: [],
    createdAt: new Date().toISOString(),
  };
};

const saveSessionData = (data: SessionData) => {
  try {
    sessionStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(data));
  } catch (error) {
    console.error('Failed to save session data:', error);
  }
};

// Initialize
const initialSessionData = getSessionData();

// Atoms
export const sessionIdAtom = atom(initialSessionData.sessionId);
const baseMessagesAtom = atom<StoredMessage[]>(initialSessionData.messages);

export const messagesAtom = atom(
  (get) => get(baseMessagesAtom),
  (get, set, newMessages: StoredMessage[]) => {
    set(baseMessagesAtom, newMessages);
    const currentSessionId = get(sessionIdAtom);
    saveSessionData({
      sessionId: currentSessionId,
      messages: newMessages,
      createdAt: initialSessionData.createdAt,
    });
  },
);

// Reset session - generates new session ID and clears messages
export const resetSessionAtom = atom(null, (get, set) => {
  const newSessionData = {
    sessionId: generateSessionId(),
    messages: [],
    createdAt: new Date().toISOString(),
  };

  saveSessionData(newSessionData);
  set(sessionIdAtom, newSessionData.sessionId);
  set(baseMessagesAtom, []);
});

// Atom for external query requests (from search bar, etc.)
export const pendingQueryAtom = atom<string | null>(null);

// Atom for chatbot open state with session storage persistence
export const isChatbotOpenAtom = atomWithStorage(
  CHATBOT_OPEN_STORAGE_KEY,
  false,
);
