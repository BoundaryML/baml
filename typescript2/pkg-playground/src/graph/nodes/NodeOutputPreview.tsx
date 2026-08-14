// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing public component filename.
import type { BamlJsValue } from '@b/pkg-proto';
import type { FC } from 'react';
import {
  CAPTURED_VALUE_CARD_WIDTH,
  CapturedValueCard,
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
export const NODE_VALUE_PREVIEW_FOOTER_HEIGHT = 14;

export function NodeOutputPreview({
  result,
  hasResult,
  valuePreviews,
  errorMessage,
  customRenderers,
}: NodeOutputPreviewProps) {
  const values = graphPreviewValues(
    valuePreviews,
    result,
    hasResult,
    errorMessage,
  );
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
        width: NODE_VALUE_PREVIEW_WIDTH,
      }}
    >
      {visible.map((value) => (
        <CapturedValueCard
          compact
          customRenderers={customRenderers}
          key={value.id}
          preserveDiagnosticLines
          prettyPrintValue
          value={value}
        />
      ))}
      {remaining > 0 ? (
        <div
          style={{
            color: '#a1a1aa',
            fontSize: 10,
            fontWeight: 600,
            lineHeight: `${NODE_VALUE_PREVIEW_FOOTER_HEIGHT}px`,
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
        diagnostic: errorMessage,
        id: 'node-error',
        label: 'error',
        role: 'callError',
        state: 'error',
        timestampMs: 0,
        value: null,
        valueRef: null,
      },
    ];
  }

  if (hasResult) {
    return [
      {
        diagnostic: null,
        id: 'node-result',
        label: 'output',
        role: 'callOutput',
        state: result == null ? 'unavailable' : 'available',
        timestampMs: 0,
        value: result ?? null,
        valueRef: null,
      },
    ];
  }

  return [];
}
