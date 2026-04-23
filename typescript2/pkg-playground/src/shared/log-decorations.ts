import type { BamlOutboundValue } from '@b/pkg-proto';
import type { LogLevel } from '../worker-protocol';

/** Format a BamlOutboundValue to a short string for inline display. */
export function formatValueShort(holder: BamlOutboundValue | null | undefined): string {
  if (!holder?.value) return 'null';

  switch (holder.value.$case) {
    case 'nullValue':
      return 'null';
    case 'stringValue':
      return JSON.stringify(holder.value.stringValue);
    case 'intValue':
      return String(holder.value.intValue);
    case 'floatValue':
      return String(holder.value.floatValue);
    case 'boolValue':
      return String(holder.value.boolValue);
    case 'classValue': {
      const cls = holder.value.classValue;
      const name = cls.name?.name ?? 'Class';
      const fields = cls.fields
        .map((f) => `${f.key}: ${formatValueShort(f.value)}`)
        .join(', ');
      return `${name} { ${fields} }`;
    }
    case 'enumValue':
      return holder.value.enumValue.value;
    case 'listValue':
      return `[${holder.value.listValue.items.map(formatValueShort).join(', ')}]`;
    case 'mapValue': {
      const entries = holder.value.mapValue.entries
        .map((e) => `${e.key}: ${formatValueShort(e.value)}`)
        .join(', ');
      return `{${entries}}`;
    }
    case 'literalValue': {
      const lit = holder.value.literalValue;
      if (!lit.literal) return 'null';
      switch (lit.literal.$case) {
        case 'stringLiteral':
          return JSON.stringify(lit.literal.stringLiteral.value);
        case 'intLiteral':
          return String(lit.literal.intLiteral.value);
        case 'boolLiteral':
          return String(lit.literal.boolLiteral.value);
        default:
          return 'null';
      }
    }
    case 'unionVariantValue':
      return formatValueShort(holder.value.unionVariantValue.value);
    case 'checkedValue':
      return formatValueShort(holder.value.checkedValue.value);
    case 'streamingStateValue':
      return formatValueShort(holder.value.streamingStateValue.value);
    case 'handleValue': {
      const h = holder.value.handleValue;
      return `<handle #${h.key}>`;
    }
    case 'mediaValue': {
      const m = holder.value.mediaValue;
      return `<${m.mimeType ?? 'media'}>`;
    }
    default:
      return '?';
  }
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
