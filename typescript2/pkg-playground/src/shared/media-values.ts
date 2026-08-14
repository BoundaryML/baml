import type { BamlJsMedia } from '@b/pkg-proto';
import { getBamlType } from '../result-renderers';

export function isBamlMedia(value: unknown): value is BamlJsMedia {
  return getBamlType(value) === '$media';
}

export function mediaToSrc(media: BamlJsMedia): string | null {
  switch (media.content_type) {
    case 'url':
      return media.url;
    case 'base64': {
      const mime = media.mime_type ?? 'application/octet-stream';
      return `data:${mime};base64,${media.base64}`;
    }
    case 'file':
      return null;
    default:
      return null;
  }
}

export function mediaLabel(media: BamlJsMedia): string {
  return media.mime_type
    ? `${media.media_type} (${media.mime_type})`
    : media.media_type;
}

export function findImageMedia(
  value: unknown,
  options: { maxDepth?: number; maxItems?: number } = {},
): BamlJsMedia[] {
  const maxDepth = options.maxDepth ?? 8;
  const maxItems = options.maxItems ?? 24;
  const images: BamlJsMedia[] = [];
  const seen = new WeakSet<object>();

  function visit(current: unknown, depth: number) {
    if (images.length >= maxItems || current == null || depth > maxDepth)
      return;

    if (isBamlMedia(current)) {
      if (current.media_type === 'image') images.push(current);
      return;
    }

    if (typeof current !== 'object') return;
    if (ArrayBuffer.isView(current)) return;
    if (seen.has(current)) return;
    seen.add(current);

    if (Array.isArray(current)) {
      for (const item of current) visit(item, depth + 1);
      return;
    }

    for (const [key, child] of Object.entries(
      current as Record<string, unknown>,
    )) {
      if (key === '$baml') continue;
      visit(child, depth + 1);
    }
  }

  visit(value, 0);
  return images;
}
