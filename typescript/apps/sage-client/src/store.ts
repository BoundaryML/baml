import { atom } from 'jotai';
import { type UserMessage, type AssistantMessage } from '@baml/sage-interface';

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

// Session storage functions
const getSessionStorageMessages = (): StoredMessage[] => {
  try {
    const stored = sessionStorage.getItem('baml-chat-messages');
    if (stored) {
      const parsed = JSON.parse(stored);
      console.log('Loading messages from session storage:', parsed.length);
      // Convert timestamp strings back to Date objects
      return parsed.map((msg: any) => ({
        ...msg,
        timestamp: new Date(msg.timestamp)
      }));
    }
  } catch (error) {
    console.error('Failed to load messages from session storage:', error);
  }
  console.log('No messages found in session storage, starting fresh');
  return [];
};

const setSessionStorageMessages = (messages: StoredMessage[]) => {
  try {
    console.log('Saving messages to session storage:', messages.length);
    sessionStorage.setItem('baml-chat-messages', JSON.stringify(messages));
  } catch (error) {
    console.error('Failed to save messages to session storage:', error);
  }
};

// Base atom for messages
const baseMessagesAtom = atom<StoredMessage[]>(getSessionStorageMessages());

// Derived atom that persists to session storage on write
export const messagesAtom = atom(
  (get) => get(baseMessagesAtom),
  (get, set, newMessages: StoredMessage[]) => {
    set(baseMessagesAtom, newMessages);
    setSessionStorageMessages(newMessages);
  }
);