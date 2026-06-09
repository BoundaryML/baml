/**
 * Renders a $media value — shows images inline, other types as a labelled badge.
 */

import type { FC } from 'react';
import type { ResultRendererProps } from '../result-renderers';
import { Badge } from '../components/ui/badge';
import { CodeBlock } from '../components/ui/code-block';
import { isBamlMedia, mediaLabel, mediaToSrc } from '../shared/media-values';

export const MediaRenderer: FC<ResultRendererProps> = ({ value, displayMode }) => {
  if (!isBamlMedia(value)) {
    return <CodeBlock>{JSON.stringify(value, null, 2)}</CodeBlock>;
  }

  if (displayMode === 'inline') {
    return <span className="font-vsc-mono text-xs text-vsc-text-faint">&lt;{value.media_type}&gt;</span>;
  }

  const src = mediaToSrc(value);
  const label = mediaLabel(value);

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
