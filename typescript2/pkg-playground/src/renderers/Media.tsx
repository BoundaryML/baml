/**
 * Renders a $media value — shows images inline, other types as a labelled badge.
 */

import type { FC } from 'react';
import type { BamlJsMedia } from '@b/pkg-proto';
import type { ResultRendererProps } from '../result-renderers';

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

const badgeCls =
  'inline-flex items-center gap-1 px-2 py-0.5 rounded border border-vsc-border bg-vsc-surface text-vsc-text-muted font-vsc-mono text-[11px]';

const codeBlockCls =
  'whitespace-pre-wrap break-all font-vsc-mono text-xs leading-relaxed p-2 rounded bg-vsc-bg border border-vsc-border text-vsc-text overflow-auto max-h-[200px] m-0';

export const MediaRenderer: FC<ResultRendererProps> = ({ value }) => {
  if (!isMedia(value)) {
    return <pre className={codeBlockCls}>{JSON.stringify(value, null, 2)}</pre>;
  }

  const src = getMediaSrc(value);
  const label = value.mime_type
    ? `${value.media_type} (${value.mime_type})`
    : value.media_type;

  if (value.media_type === 'image' && src) {
    return (
      <div className="space-y-1">
        <span className={badgeCls}>{label}</span>
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
        <span className={badgeCls}>{label}</span>
        <audio controls src={src} className="w-full" />
      </div>
    );
  }

  if (value.media_type === 'video' && src) {
    return (
      <div className="space-y-1">
        <span className={badgeCls}>{label}</span>
        <video controls src={src} className="max-w-full max-h-[300px] rounded border border-vsc-border" />
      </div>
    );
  }

  // File reference or unsupported content_type — show badge with path/url
  const ref = value.content_type === 'url' ? value.url
    : value.content_type === 'file' ? value.file
    : '(base64)';
  return (
    <div className={badgeCls}>
      {label}: {ref}
    </div>
  );
};
