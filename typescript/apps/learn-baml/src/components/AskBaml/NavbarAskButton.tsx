import React from 'react';
import { Sparkles } from 'lucide-react';
import { useChat } from './hooks/useChat';
import styles from './NavbarAskButton.module.css';

export function NavbarAskButton() {
  const { isOpen, setIsOpen } = useChat();

  return (
    <button
      className={styles.button}
      onClick={() => setIsOpen(!isOpen, 'button')}
      aria-label="Ask AI Assistant (Cmd+K)"
      data-active={isOpen}
    >
      <Sparkles size={14} className={styles.icon} />
      <span className={styles.label}>Ask AI</span>
      <kbd className={styles.shortcut}>
        <span className={styles.shortcutKey}>⌘</span>K
      </kbd>
    </button>
  );
}
