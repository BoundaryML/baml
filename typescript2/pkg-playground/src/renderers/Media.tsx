/**
 * Renders a $media value — shows images inline, other types as a labelled badge.
 */

import type { FC } from 'react';
import type { BamlJsMedia } from '@b/pkg-proto';
import type { ResultRendererProps } from '../result-renderers';
import { Badge } from '../components/ui/badge';
import { CodeBlock } from '../components/ui/code-block';

function isMedia(value: unknown): value is BamlJsMedia {
  if (value == null || typeof value !== 'object') return false;
  const baml = (value as Record<string, unknown>).$baml;
  if (baml == null || typeof baml !== 'object') return false;
  return (baml as Record<string, unknown>).type === '$media';
}

function getMediaSrc(m: BamlJsMedia): string | null {
  if (m.content_type === 'url') return m.url;
  if (m.content_type === 'base64') {
    const mime = m.mime_type ?? 'application/octet-stream';
    return `data:${mime};base64,${m.base64}`;
  }
  return null;
}

export const MediaRenderer: FC<ResultRendererProps> = ({ value }) => {
  if (!isMedia(value)) {
    return <CodeBlock>{JSON.stringify(value, null, 2)}</CodeBlock>;
  }

  const src = getMediaSrc(value);
  const label = value.mime_type
    ? `${value.media_type} (${value.mime_type})`
    : value.media_type;

  if (value.media_type === 'image' && src) {
    return (
      <div className="space-y-1">
        <Badge variant="secondary" className="gap-1 text-[11px] font-vsc-mono">{label}</Badge>
        <img
          src={src}
          alt="media"
          className="max-w-full max-h-[300px] rounded border border-vsc-border"
        />
      </div>
    );
  }

  if (value.media_type === 'audio' && src) {
    return (
      <div className="space-y-1">
        <Badge variant="secondary" className="gap-1 text-[11px] font-vsc-mono">{label}</Badge>
        <audio controls src={src} className="w-full" />
      </div>
    );
  }

  if (value.media_type === 'video' && src) {
    return (
      <div className="space-y-1">
        <Badge variant="secondary" className="gap-1 text-[11px] font-vsc-mono">{label}</Badge>
        <video controls src={src} className="max-w-full max-h-[300px] rounded border border-vsc-border" />
      </div>
    );
  }

  // File reference or unsupported content_type — show badge with path/url
  const ref = value.content_type === 'url' ? value.url
    : value.content_type === 'file' ? value.file
    : '(base64)';
  return (
    <Badge variant="secondary" className="gap-1 text-[11px] font-vsc-mono">
      {label}: {ref}
    </Badge>
  );
};
