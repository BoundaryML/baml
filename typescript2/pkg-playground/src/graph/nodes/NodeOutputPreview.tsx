import type { FC } from 'react';
import type { BamlJsMedia, BamlJsValue } from '@b/pkg-proto';
import type { ResultRendererProps } from '../../result-renderers';
import { mediaToSrc } from '../../shared/media-values';
import { ValueRenderer } from '../../ValueRenderer';

interface NodeOutputPreviewProps {
  result?: BamlJsValue | null;
  hasResult?: boolean;
  images?: BamlJsMedia[];
  errorMessage?: string | null;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}

export function NodeOutputPreview({
  result,
  hasResult,
  images,
  errorMessage,
  customRenderers,
}: NodeOutputPreviewProps) {
  if (errorMessage) {
    return (
      <div
        className="nodrag nopan"
        style={{
          marginTop: 6,
          maxWidth: 320,
          maxHeight: 180,
          overflow: 'auto',
          borderRadius: 6,
          border: '1px solid rgba(244,63,94,0.35)',
          background: 'rgba(64,21,30,0.72)',
          padding: 8,
        }}
      >
        <div style={{ color: '#fda4af', fontSize: 10, fontWeight: 700, marginBottom: 4, textTransform: 'uppercase' }}>
          Error
        </div>
        <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', color: '#fecdd3', fontSize: 10, lineHeight: 1.35 }}>
          {errorMessage}
        </pre>
      </div>
    );
  }

  if (hasResult) {
    return (
      <div
        className="nodrag nopan"
        style={{
          marginTop: 6,
          maxWidth: 320,
          maxHeight: 220,
          overflow: 'auto',
          borderRadius: 6,
          border: '1px solid rgba(255,255,255,0.10)',
          background: 'rgba(15,23,42,0.72)',
          padding: 8,
        }}
      >
        <div style={{ color: '#9ca3af', fontSize: 10, fontWeight: 700, marginBottom: 4, textTransform: 'uppercase' }}>
          Output
        </div>
        <ValueRenderer value={result} displayMode="expanded" customRenderers={customRenderers} />
      </div>
    );
  }

  if (!images || images.length === 0) return null;

  const visible = images.slice(0, 4);
  const remaining = images.length - visible.length;

  return (
    <div
      className="nodrag nopan"
      style={{
        display: 'grid',
        gridTemplateColumns: visible.length === 1 ? '1fr' : 'repeat(2, minmax(0, 1fr))',
        gap: 4,
        marginTop: 6,
      }}
    >
      {visible.map((image, index) => {
        const src = mediaToSrc(image);
        const isLastWithRemainder = index === visible.length - 1 && remaining > 0;

        return (
          <div
            key={`${image.content_type}-${index}`}
            style={{
              position: 'relative',
              width: '100%',
              height: visible.length === 1 ? 92 : 56,
              borderRadius: 4,
              overflow: 'hidden',
              background: '#111827',
              border: '1px solid rgba(255,255,255,0.12)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            {src ? (
              <img
                src={src}
                alt="BAML image output"
                loading="lazy"
                style={{
                  width: '100%',
                  height: '100%',
                  objectFit: 'cover',
                  display: 'block',
                }}
              />
            ) : (
              <span style={{ color: '#9ca3af', fontSize: 10, fontFamily: 'monospace' }}>
                &lt;image&gt;
              </span>
            )}
            {isLastWithRemainder && (
              <div
                style={{
                  position: 'absolute',
                  inset: 0,
                  background: 'rgba(0,0,0,0.58)',
                  color: '#fff',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  fontSize: 12,
                  fontWeight: 700,
                }}
              >
                +{remaining}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
