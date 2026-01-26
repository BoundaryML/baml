import type { WatchNotification } from '../types'

export function parseNotificationValue(value: string | undefined): any {
  if (value === undefined) return undefined;
  try {
    // Try to parse as JSON first
    return JSON.parse(value);
  } catch {
    // If not JSON, return as is
    return value;
  }
}

export function getNotificationLogFilterKey(notification: WatchNotification): string | undefined {
  return notification.stateUpdate?.logFilterKey;
}

export function getNotificationLabel(notification: WatchNotification): string {
  if (notification.variableName) {
    return notification.variableName;
  }

  // Try to parse JSON value to check for block type
  try {
    if (notification.value) {
      const parsed = JSON.parse(notification.value);
      if (parsed.type === 'block' && parsed.label) {
        return parsed.label;
      }
    }
  } catch {
    // Fall back to old format if not JSON
    if (notification.value?.startsWith('Block(')) {
      // Extract block name from "Block(name)" format
      const match = notification.value?.match(/Block\("(.+?)"\)/);
      return match ? `Block: ${match[1]}` : 'Block';
    }
  }

  const blockKey = getNotificationLogFilterKey(notification);
  if (notification.isStream) {
    return `Stream: ${blockKey ?? 'unknown'}`;
  }
  return blockKey ?? 'Block';
}

export function getNotificationType(notification: WatchNotification): 'variable' | 'block' | 'stream' {
  // Try to parse JSON value to check type
  try {
    if (notification.value) {
      const parsed = JSON.parse(notification.value);
      if (parsed.type === 'block') return 'block';
      if (parsed.type && parsed.type.startsWith('stream')) return 'stream';
    }
  } catch {
    // Fall back to old format if not JSON
    if (notification.value?.startsWith('Block(')) return 'block';
  }

  if (notification.isStream) return 'stream';
  return 'variable';
}
