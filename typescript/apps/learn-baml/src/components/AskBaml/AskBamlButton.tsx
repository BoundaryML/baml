import React from 'react';
import { useChat } from './hooks/useChat';
import styles from './styles.module.css';

export function AskBamlButton() {
  const { isOpen, setIsOpen } = useChat();

  if (isOpen) return null;

  return (
    <button
      className={styles.floatingButton}
      onClick={() => setIsOpen(true, 'button')}
      aria-label="Ask BAML Assistant (Cmd+K)"
    >
      <span className={styles.buttonIcon}>?</span>
      <span className={styles.buttonLabel}>Ask AI</span>
      <span className={styles.buttonShortcut}>Cmd+K</span>
    </button>
  );
}
