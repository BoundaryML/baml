import type { FC } from 'react';
import type { BamlJsValue } from '@b/pkg-proto';
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
export const CAPTURED_VALUE_CARD_FIXED_HEIGHT =
  CAPTURED_VALUE_CARD_HEADER_HEIGHT +
  CAPTURED_VALUE_CARD_PADDING_Y +
  CAPTURED_VALUE_CARD_TEXT_HEIGHT;

const ROLE_LABELS: Record<RunTraceCallValue['role'], string> = {
  callInput: 'Input',
  callOutput: 'Output',
  callError: 'Error',
};

const ROLE_COLORS: Record<RunTraceCallValue['role'], string> = {
  callInput: '#a1a1aa',
  callOutput: '#7dd3fc',
  callError: '#fda4af',
};

const STATE_LABELS: Partial<Record<RunTraceCallValue['state'], string>> = {
  loading: 'loading',
  pending: 'pending',
  omitted: 'omitted',
  truncated: 'truncated',
  missing: 'missing',
  lost: 'lost',
  error: 'error',
  unavailable: 'unavailable',
};

interface CapturedValueCardProps {
  value: RunTraceCallValue;
  compact?: boolean;
  fixedHeight?: boolean;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}

export function CapturedValueCard({
  value,
  compact = false,
  fixedHeight = false,
  customRenderers,
}: CapturedValueCardProps) {
  const images = valueToImagePreviews(value.value);
  const visibleImages = images.slice(0, CAPTURED_VALUE_CARD_MAX_IMAGES);
  const remainingImages = images.length - visibleImages.length;
  const fixedImageHeight =
    visibleImages.length <= 2
      ? CAPTURED_VALUE_CARD_TEXT_HEIGHT
      : (CAPTURED_VALUE_CARD_TEXT_HEIGHT - CAPTURED_VALUE_CARD_IMAGE_GAP) / 2;
  const stateLabel =
    value.state === 'available' ? null : STATE_LABELS[value.state];
  const roleColor = ROLE_COLORS[value.role];
  const isError = value.role === 'callError' || value.state === 'error';

  return (
    <div
      className="nodrag nopan"
      style={{
        borderRadius: 6,
        border: `1px solid ${
          isError ? 'rgba(244,63,94,0.35)' : 'rgba(255,255,255,0.10)'
        }`,
        background: isError ? 'rgba(64,21,30,0.72)' : 'rgba(15,23,42,0.72)',
        padding: compact ? 7 : 8,
        boxSizing: 'border-box',
        height: fixedHeight ? CAPTURED_VALUE_CARD_FIXED_HEIGHT : undefined,
        overflow: 'hidden',
      }}
      title={value.diagnostic ?? undefined}
    >
      <div
        style={{
          display: 'flex',
          minWidth: 0,
          alignItems: 'center',
          gap: 6,
          minHeight: 13,
        }}
      >
        <span
          style={{
            borderRadius: 4,
            border: `1px solid ${roleColor}55`,
            color: roleColor,
            padding: '1px 4px',
            fontSize: 9,
            fontWeight: 700,
            textTransform: 'uppercase',
            lineHeight: 1.4,
            flexShrink: 0,
          }}
        >
          {ROLE_LABELS[value.role]}
        </span>
        {value.label ? (
          <span
            style={{
              minWidth: 0,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              color: '#a1a1aa',
              fontSize: 10,
              lineHeight: 1.4,
            }}
          >
            {value.label}
          </span>
        ) : null}
        {stateLabel ? (
          <span
            style={{
              marginLeft: 'auto',
              flexShrink: 0,
              borderRadius: 4,
              border: '1px solid rgba(255,255,255,0.10)',
              color: '#a1a1aa',
              padding: '1px 4px',
              fontSize: 9,
              fontWeight: 600,
              lineHeight: 1.4,
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
            gridTemplateColumns:
              visibleImages.length === 1 ? '1fr' : 'repeat(2, minmax(0, 1fr))',
            gap: CAPTURED_VALUE_CARD_IMAGE_GAP,
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
                  position: 'relative',
                  width: '100%',
                  height: fixedHeight
                    ? fixedImageHeight
                    : visibleImages.length === 1
                      ? CAPTURED_VALUE_CARD_SINGLE_IMAGE_HEIGHT
                      : CAPTURED_VALUE_CARD_TILE_IMAGE_HEIGHT,
                  borderRadius: 6,
                  overflow: 'hidden',
                  background: '#09090b',
                  border: '1px solid rgba(255,255,255,0.14)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                {src ? (
                  <img
                    src={src}
                    alt="BAML captured image"
                    loading="lazy"
                    style={{
                      width: '100%',
                      height: '100%',
                      objectFit: 'contain',
                      display: 'block',
                    }}
                  />
                ) : (
                  <span
                    style={{
                      color: '#9ca3af',
                      fontSize: 10,
                      fontFamily:
                        'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
                    }}
                  >
                    &lt;image&gt;
                  </span>
                )}
                {isLastWithRemainder ? (
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
            marginTop: 6,
            maxHeight: compact ? CAPTURED_VALUE_CARD_TEXT_HEIGHT : 180,
            overflow: 'auto',
            color: isError ? '#fecdd3' : '#e5e7eb',
            fontSize: 10,
          }}
        >
          <ValueRenderer
            value={value.value}
            displayMode={compact ? 'inline' : 'expanded'}
            customRenderers={customRenderers}
          />
        </div>
      ) : null}
      {value.diagnostic ? (
        <div
          style={{
            marginTop: 5,
            color: '#a1a1aa',
            fontSize: 10,
            lineHeight: 1.35,
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
