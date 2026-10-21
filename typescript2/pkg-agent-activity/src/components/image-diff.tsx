// A change to an image, shown as the image: what it was, what it became, or
// both side by side.
//
// While the edit plays, the new image opens from the top in bands, the way
// added code rows open one after another, and a deleted image drains to grey.
// Tiny images (icons) are scaled up with hard pixel edges so they can be seen.

import { type CSSProperties, memo, useMemo, useState } from 'react';
import type { ActiveEdit } from '../engine/player';
import type { MediaRef } from '../engine/protocol';

/** Images at most this many pixels on each side are scaled up. */
const SMALL_PX = 64;

export interface ImageDiffProps {
  /** File name, for the images' alt text. */
  name: string;
  before: MediaRef | null;
  after: MediaRef | null;
  /** Size of the new file in bytes, when known. */
  size?: number | null;
  /** The playing edit's timing, when this is it. */
  playing?: ActiveEdit | null;
}

export const ImageDiff = memo(function ImageDiff({
  name,
  before,
  after,
  size,
  playing,
}: ImageDiffProps) {
  // Fixed per play, so a re-render midway does not move the animation.
  const timing = useMemo(() => {
    if (!playing) return undefined;
    const elapsed = performance.now() - playing.startedAt;
    return {
      '--aa-img-delay': `${Math.round(playing.travelMs - elapsed)}ms`,
      '--aa-img-dur': `${Math.round(Math.max(240, playing.writeMs))}ms`,
    } as CSSProperties;
  }, [playing]);
  const kind = before && after ? 'modified' : after ? 'created' : 'deleted';
  return (
    <div
      className="aa-image-diff"
      data-kind={kind}
      data-playing={playing ? 'true' : undefined}
      key={playing?.startedAt}
      style={timing}
    >
      {before && (
        <Picture
          alt={`${name}, ${after ? 'before this edit' : 'deleted'}`}
          key={before.url}
          label={after ? 'Before' : 'Deleted'}
          media={before}
          side="before"
        />
      )}
      {after && (
        <Picture
          alt={before ? `${name}, after this edit` : name}
          key={after.url}
          label={before ? 'After' : 'New'}
          media={after}
          side="after"
          size={size}
        />
      )}
    </div>
  );
});

function Picture({
  media,
  side,
  label,
  alt,
  size,
}: {
  media: MediaRef;
  side: 'before' | 'after';
  label: string;
  alt: string;
  size?: number | null;
}) {
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  const [failed, setFailed] = useState(false);
  const small = dims !== null && dims.w <= SMALL_PX && dims.h <= SMALL_PX;
  return (
    <figure className="aa-picture" data-side={side}>
      <div className="aa-picture-frame">
        {failed ? (
          <span className="aa-picture-missing">
            {media.url.startsWith('data:')
              ? 'This SVG does not render.'
              : 'This version is no longer kept.'}
          </span>
        ) : (
          <img
            alt={alt}
            data-small={small ? 'true' : undefined}
            draggable={false}
            onError={() => setFailed(true)}
            onLoad={(e) =>
              setDims({
                h: e.currentTarget.naturalHeight,
                w: e.currentTarget.naturalWidth,
              })
            }
            src={media.url}
          />
        )}
      </div>
      <figcaption>
        <span className="label">{label}</span>
        {dims && dims.w > 0 && (
          <span>
            {dims.w}×{dims.h}
          </span>
        )}
        {size != null && <span>{formatBytes(size)}</span>}
      </figcaption>
    </figure>
  );
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
