/**
 * Navigation Logger
 *
 * Records all navigation events for debugging
 */

import type { NavigationLogEntry, NavigationInput } from './types';

export class NavigationLogger {
  private logs: NavigationLogEntry[] = [];
  private readonly maxLogs = 100;

  /**
   * Log a navigation event
   */
  log(entry: NavigationLogEntry): void {
    this.logs.push(entry);

    // Keep only last maxLogs entries
    if (this.logs.length > this.maxLogs) {
      this.logs.shift();
    }

    // Console output with collapsed group
    console.groupCollapsed(
      `%cNav: ${entry.from.mode} → ${entry.to.mode} (${entry.rule})`,
      'color: #00aa00; font-weight: bold'
    );
    console.log('Input:', entry.input);
    console.log('Target:', entry.target);
    console.log('Rule:', entry.rule);
    console.log('From:', entry.from);
    console.log('To:', entry.to);
    console.log('Effects:', entry.effects);
    console.log('Duration:', `${entry.duration.toFixed(2)}ms`);
    console.groupEnd();
  }

  /**
   * Log a navigation error
   */
  error(input: NavigationInput, error: Error): void {
    console.error('Navigation failed', { input, error });
  }

  /**
   * Get navigation history
   */
  getHistory(): NavigationLogEntry[] {
    return [...this.logs];
  }

  /**
   * Export logs as JSON
   */
  export(): string {
    return JSON.stringify(this.logs, null, 2);
  }

  /**
   * Clear all logs
   */
  clear(): void {
    this.logs = [];
  }
}

// Create singleton logger
export const navLogger = new NavigationLogger();

// Expose for debugging in browser console
if (typeof window !== 'undefined') {
  (window as any).__navLogs = () => navLogger.getHistory();
  (window as any).__navExport = () => navLogger.export();
  (window as any).__navClear = () => navLogger.clear();
}
