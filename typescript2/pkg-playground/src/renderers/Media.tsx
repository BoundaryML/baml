// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing exported component path.
/**
 * Renders a $media value — shows images inline, other types as a labelled badge.
 */

import type { FC } from 'react';
import { Badge } from '../components/ui/badge';
import { CodeBlock } from '../components/ui/code-block';
import type { ResultRendererProps } from '../result-renderers';
import { isBamlMedia, mediaLabel, mediaToSrc } from '../shared/media-values';

export const MediaRenderer: FC<ResultRendererProps> = ({
  value,
  displayMode,
}) => {
  if (!isBamlMedia(value)) {
    return <CodeBlock>{JSON.stringify(value, null, 2)}</CodeBlock>;
  }

  if (displayMode === 'inline') {
    return (
      <span className="font-vsc-mono text-xs text-vsc-text-faint">
        &lt;{value.media_type}&gt;
      </span>
    );
  }

  const src = mediaToSrc(value);
  const label = mediaLabel(value);

  if (value.media_type === 'image' && src) {
    return (
      <div className="space-y-1">
        <Badge className="gap-1 text-[11px] font-vsc-mono" variant="secondary">
          {label}
        </Badge>
        <img
          alt="media"
          className="max-w-full max-h-[300px] rounded border border-vsc-border"
          src={src}
        />
      </div>
    );
  }

  if (value.media_type === 'audio' && src) {
    return (
      <div className="space-y-1">
        <Badge className="gap-1 text-[11px] font-vsc-mono" variant="secondary">
          {label}
        </Badge>
        {/* biome-ignore lint/a11y/useMediaCaption: a BAML media value carries no captions */}
        <audio className="w-full" controls src={src} />
      </div>
    );
  }

  if (value.media_type === 'video' && src) {
    return (
      <div className="space-y-1">
        <Badge className="gap-1 text-[11px] font-vsc-mono" variant="secondary">
          {label}
        </Badge>
        {/* biome-ignore lint/a11y/useMediaCaption: a BAML media value carries no captions */}
        <video
          className="max-w-full max-h-[300px] rounded border border-vsc-border"
          controls
          src={src}
        />
      </div>
    );
  }

  // PDFs and other media are not shown inline — show badge with url/name
  const ref =
    value.content_type === 'url' ? value.url : (value.name ?? '(base64)');
  return (
    <Badge className="gap-1 text-[11px] font-vsc-mono" variant="secondary">
      {label}: {ref}
    </Badge>
  );
};
