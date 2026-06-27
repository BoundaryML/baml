import type { FC } from 'react';
import type { BamlJsValue } from '@b/pkg-proto';
import {
  CapturedValueCard,
  CAPTURED_VALUE_CARD_WIDTH,
} from '../../CapturedValueCard';
import type { ResultRendererProps } from '../../result-renderers';
import type { GraphNodeValuePreview } from '../../run-store-projections';

interface NodeOutputPreviewProps {
  result?: BamlJsValue | null;
  hasResult?: boolean;
  valuePreviews?: GraphNodeValuePreview[];
  errorMessage?: string | null;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}

export const NODE_VALUE_PREVIEW_MAX = 4;
export const NODE_VALUE_PREVIEW_WIDTH = CAPTURED_VALUE_CARD_WIDTH;
export const NODE_VALUE_PREVIEW_GAP = 6;

export function NodeOutputPreview({
  result,
  hasResult,
  valuePreviews,
  errorMessage,
  customRenderers,
}: NodeOutputPreviewProps) {
  const values = graphPreviewValues(valuePreviews, result, hasResult, errorMessage);
  if (values.length === 0) return null;

  const visible = values.slice(0, NODE_VALUE_PREVIEW_MAX);
  const remaining = values.length - visible.length;

  return (
    <div
      className="nodrag nopan"
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: NODE_VALUE_PREVIEW_GAP,
        marginTop: 6,
        width: '100%',
        maxWidth: NODE_VALUE_PREVIEW_WIDTH,
      }}
    >
      {visible.map((value) => (
        <CapturedValueCard
          key={value.id}
          value={value}
          compact
          customRenderers={customRenderers}
        />
      ))}
      {remaining > 0 ? (
        <div
          style={{
            color: '#a1a1aa',
            fontSize: 10,
            fontWeight: 600,
            paddingLeft: 2,
          }}
        >
          +{remaining} more
        </div>
      ) : null}
    </div>
  );
}

function graphPreviewValues(
  valuePreviews: GraphNodeValuePreview[] | undefined,
  result: BamlJsValue | null | undefined,
  hasResult: boolean | undefined,
  errorMessage: string | null | undefined,
): GraphNodeValuePreview[] {
  if (valuePreviews && valuePreviews.length > 0) return valuePreviews;

  if (errorMessage) {
    return [
      {
        id: 'node-error',
        timestampMs: 0,
        role: 'callError',
        label: 'error',
        valueRef: null,
        value: null,
        state: 'error',
        diagnostic: errorMessage,
      },
    ];
  }

  if (hasResult) {
    return [
      {
        id: 'node-result',
        timestampMs: 0,
        role: 'callOutput',
        label: 'output',
        valueRef: null,
        value: result ?? null,
        state: result == null ? 'unavailable' : 'available',
        diagnostic: null,
      },
    ];
  }

  return [];
}
