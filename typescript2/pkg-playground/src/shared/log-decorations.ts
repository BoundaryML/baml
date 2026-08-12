import type { LogLevel } from '../worker-protocol';

/** Truncate a message to max length with ellipsis. */
export function truncateMessage(msg: string, maxLen: number = 60): string {
  if (msg.length <= maxLen) return msg;
  return msg.slice(0, maxLen - 1) + '\u2026';
}

/** Map log level string to our LogLevel type. */
export function normalizeLogLevel(level: string): LogLevel {
  switch (level.toLowerCase()) {
    case 'error':
      return 'error';
    case 'warn':
    case 'warning':
      return 'warn';
    case 'debug':
      return 'debug';
    default:
      return 'info';
  }
}
