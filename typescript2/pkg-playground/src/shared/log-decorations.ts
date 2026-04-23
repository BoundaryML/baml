import type { BamlJsValue } from '@b/pkg-proto';
import { getBamlType } from '../result-renderers';
import type { LogLevel } from '../worker-protocol';

/** Format a BamlJsValue to a short string for inline display. */
export function formatValueShort(value: BamlJsValue | null | undefined): string {
  if (value == null) return 'null';
  if (typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'number') return String(value);
  if (typeof value === 'boolean') return String(value);
  if (typeof value === 'bigint') return String(value);
  if (Array.isArray(value)) {
    return `[${value.map(formatValueShort).join(', ')}]`;
  }
  if (typeof value === 'object') {
    const bamlType = getBamlType(value);
    if (bamlType === '$media') {
      return `<${(value as Record<string, unknown>).mime_type ?? (value as Record<string, unknown>).media_type ?? 'media'}>`;
    }
    if (bamlType === '$handle') {
      return `<handle #${(value as Record<string, unknown>).handle_key}>`;
    }
    if (bamlType === '$prompt_ast') return '<prompt_ast>';
    if (bamlType) {
      const entries = Object.entries(value)
        .filter(([k]) => k !== '$baml')
        .map(([k, v]) => `${k}: ${formatValueShort(v as BamlJsValue)}`)
        .join(', ');
      return `${bamlType} { ${entries} }`;
    }
    const entries = Object.entries(value)
      .map(([k, v]) => `${k}: ${formatValueShort(v as BamlJsValue)}`)
      .join(', ');
    return `{${entries}}`;
  }
  return '?';
}

/** Truncate a message to max length with ellipsis. */
export function truncateMessage(msg: string, maxLen: number = 60): string {
  if (msg.length <= maxLen) return msg;
  return msg.slice(0, maxLen - 1) + '\u2026';
}

/** Map log level string to our LogLevel type. */
export function normalizeLogLevel(level: string): LogLevel {
  switch (level.toLowerCase()) {
    case 'error': return 'error';
    case 'warn': case 'warning': return 'warn';
    case 'debug': return 'debug';
    default: return 'info';
  }
}
