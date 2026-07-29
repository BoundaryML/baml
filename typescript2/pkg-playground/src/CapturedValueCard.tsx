// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing public component filename.
import type { BamlJsValue } from '@b/pkg-proto';
import type { FC } from 'react';
import type { ResultRendererProps } from './result-renderers';
import type { RunTraceCallValue } from './run-store-projections';
import { findImageMedia, mediaToSrc } from './shared/media-values';
import { ValueRenderer } from './ValueRenderer';

export const CAPTURED_VALUE_CARD_WIDTH = 360;
export const CAPTURED_VALUE_CARD_MAX_IMAGES = 4;
export const CAPTURED_VALUE_CARD_IMAGE_GAP = 6;
export const CAPTURED_VALUE_CARD_SINGLE_IMAGE_HEIGHT = 240;
export const CAPTURED_VALUE_CARD_TILE_IMAGE_HEIGHT = 126;
export const CAPTURED_VALUE_CARD_TEXT_HEIGHT = 96;
export const CAPTURED_VALUE_CARD_HEADER_HEIGHT = 21;
export const CAPTURED_VALUE_CARD_PADDING_Y = 16;

const ROLE_LABELS: Record<RunTraceCallValue['role'], string> = {
  callError: 'Error',
  callInput: 'Input',
  callOutput: 'Output',
};

const ROLE_COLORS: Record<RunTraceCallValue['role'], string> = {
  callError: '#fda4af',
  callInput: '#a1a1aa',
  callOutput: '#7dd3fc',
};

const STATE_LABELS: Partial<Record<RunTraceCallValue['state'], string>> = {
  error: 'error',
  loading: 'loading',
  lost: 'lost',
  missing: 'missing',
  omitted: 'omitted',
  pending: 'pending',
  truncated: 'truncated',
  unavailable: 'unavailable',
};

interface CapturedValueCardProps {
  value: RunTraceCallValue;
  compact?: boolean;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}

export function capturedValueCardContentHeight(
  value: RunTraceCallValue,
): number {
  const images = valueToImagePreviews(value.value);
  if (images.length > 0) {
    const visibleCount = Math.min(
      images.length,
      CAPTURED_VALUE_CARD_MAX_IMAGES,
    );
    if (visibleCount === 1) return CAPTURED_VALUE_CARD_SINGLE_IMAGE_HEIGHT;
    const rows = Math.ceil(visibleCount / 2);
    return (
      rows * CAPTURED_VALUE_CARD_TILE_IMAGE_HEIGHT +
      (rows - 1) * CAPTURED_VALUE_CARD_IMAGE_GAP
    );
  }
  if (value.value !== null) return CAPTURED_VALUE_CARD_TEXT_HEIGHT;
  return 0;
}

export function CapturedValueCard({
  value,
  compact = false,
  customRenderers,
}: CapturedValueCardProps) {
  const images = valueToImagePreviews(value.value);
  const visibleImages = images.slice(0, CAPTURED_VALUE_CARD_MAX_IMAGES);
  const remainingImages = images.length - visibleImages.length;
  const stateLabel =
    value.state === 'available' ? null : STATE_LABELS[value.state];
  const roleColor = ROLE_COLORS[value.role];
  const isError = value.role === 'callError' || value.state === 'error';

  return (
    <div
      className="nodrag nopan"
      style={{
        background: isError ? 'rgba(64,21,30,0.72)' : 'rgba(15,23,42,0.72)',
        border: `1px solid ${
          isError ? 'rgba(244,63,94,0.35)' : 'rgba(255,255,255,0.10)'
        }`,
        borderRadius: 6,
        overflow: 'hidden',
        padding: compact ? 7 : 8,
      }}
      title={value.diagnostic ?? undefined}
    >
      <div
        style={{
          alignItems: 'center',
          display: 'flex',
          gap: 6,
          minHeight: 13,
          minWidth: 0,
        }}
      >
        <span
          style={{
            border: `1px solid ${roleColor}55`,
            borderRadius: 4,
            color: roleColor,
            flexShrink: 0,
            fontSize: 9,
            fontWeight: 700,
            lineHeight: 1.4,
            padding: '1px 4px',
            textTransform: 'uppercase',
          }}
        >
          {ROLE_LABELS[value.role]}
        </span>
        {value.label ? (
          <span
            style={{
              color: '#a1a1aa',
              fontSize: 10,
              lineHeight: 1.4,
              minWidth: 0,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
            }}
          >
            {value.label}
          </span>
        ) : null}
        {stateLabel ? (
          <span
            style={{
              border: '1px solid rgba(255,255,255,0.10)',
              borderRadius: 4,
              color: '#a1a1aa',
              flexShrink: 0,
              fontSize: 9,
              fontWeight: 600,
              lineHeight: 1.4,
              marginLeft: 'auto',
              padding: '1px 4px',
            }}
          >
            {stateLabel}
          </span>
        ) : null}
      </div>
      {visibleImages.length > 0 ? (
        <div
          style={{
            display: 'grid',
            gap: CAPTURED_VALUE_CARD_IMAGE_GAP,
            gridTemplateColumns:
              visibleImages.length === 1 ? '1fr' : 'repeat(2, minmax(0, 1fr))',
            marginTop: 6,
          }}
        >
          {visibleImages.map((image, index) => {
            const src = mediaToSrc(image);
            const isLastWithRemainder =
              index === visibleImages.length - 1 && remainingImages > 0;
            return (
              <div
                key={`${image.content_type}-${image.mime_type ?? ''}-${index}`}
                style={{
                  alignItems: 'center',
                  background: '#09090b',
                  border: '1px solid rgba(255,255,255,0.14)',
                  borderRadius: 6,
                  display: 'flex',
                  height:
                    visibleImages.length === 1
                      ? CAPTURED_VALUE_CARD_SINGLE_IMAGE_HEIGHT
                      : CAPTURED_VALUE_CARD_TILE_IMAGE_HEIGHT,
                  justifyContent: 'center',
                  overflow: 'hidden',
                  position: 'relative',
                  width: '100%',
                }}
              >
                {src ? (
                  <img
                    alt="BAML captured value"
                    loading="lazy"
                    src={src}
                    style={{
                      display: 'block',
                      height: '100%',
                      objectFit: 'contain',
                      width: '100%',
                    }}
                  />
                ) : (
                  <span
                    style={{
                      color: '#9ca3af',
                      fontFamily:
                        'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
                      fontSize: 10,
                    }}
                  >
                    &lt;image&gt;
                  </span>
                )}
                {isLastWithRemainder ? (
                  <div
                    style={{
                      alignItems: 'center',
                      background: 'rgba(0,0,0,0.58)',
                      color: '#fff',
                      display: 'flex',
                      fontSize: 12,
                      fontWeight: 700,
                      inset: 0,
                      justifyContent: 'center',
                      position: 'absolute',
                    }}
                  >
                    +{remainingImages}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      ) : value.value !== null ? (
        <div
          style={{
            color: isError ? '#fecdd3' : '#e5e7eb',
            fontSize: 10,
            marginTop: 6,
            maxHeight: compact ? CAPTURED_VALUE_CARD_TEXT_HEIGHT : 180,
            overflow: 'auto',
          }}
        >
          <ValueRenderer
            customRenderers={customRenderers}
            displayMode={compact ? 'inline' : 'expanded'}
            value={value.value}
          />
        </div>
      ) : null}
      {value.diagnostic ? (
        <div
          style={{
            color: '#a1a1aa',
            fontSize: 10,
            lineHeight: 1.35,
            marginTop: 5,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {value.diagnostic}
        </div>
      ) : null}
    </div>
  );
}

function valueToImagePreviews(value: BamlJsValue | null) {
  return value == null ? [] : findImageMedia(value, { maxItems: 24 });
}
