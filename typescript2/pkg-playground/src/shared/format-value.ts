import type { BamlJsValue, PlainHandleDescriptor } from '@b/pkg-proto';
import { getBamlType } from '../result-renderers';

/**
 * Format a BamlJsValue as a compact string for non-React contexts
 * (e.g. Monaco editor inline decorations).
 */
export function formatValue(value: BamlJsValue | null | undefined, mode: 'inline-hint'): string {
  if (value == null) return 'null';
  if (typeof value === 'string') return JSON.stringify(value);
  if (typeof value === 'number') return String(value);
  if (typeof value === 'boolean') return String(value);
  if (typeof value === 'bigint') return String(value);

  if (Array.isArray(value)) {
    return `[${value.map((v) => formatValue(v, mode)).join(', ')}]`;
  }

  if (typeof value === 'object') {
    const bamlType = getBamlType(value);
    if (bamlType === '$media') {
      return `<${(value as Record<string, unknown>).mime_type ?? (value as Record<string, unknown>).media_type ?? 'media'}>`;
    }
    if (bamlType === '$handle') {
      const descriptor = (value as { handle: PlainHandleDescriptor }).handle;
      return `<handle #${descriptor.handle_key}>`;
    }
    if (bamlType === '$prompt_ast') return '<prompt_ast>';
    if (bamlType) {
      const entries = Object.entries(value)
        .filter(([k]) => k !== '$baml')
        .map(([k, v]) => `${k}: ${formatValue(v as BamlJsValue, mode)}`)
        .join(', ');
      return `${bamlType} { ${entries} }`;
    }
    // Plain map
    const entries = Object.entries(value)
      .map(([k, v]) => `${k}: ${formatValue(v as BamlJsValue, mode)}`)
      .join(', ');
    return `{${entries}}`;
  }

  return '?';
}
