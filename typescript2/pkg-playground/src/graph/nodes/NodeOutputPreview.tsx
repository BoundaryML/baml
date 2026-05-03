import type { BamlJsMedia } from '@b/pkg-proto';
import { mediaToSrc } from '../../shared/media-values';

export function NodeOutputPreview({ images }: { images?: BamlJsMedia[] }) {
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
