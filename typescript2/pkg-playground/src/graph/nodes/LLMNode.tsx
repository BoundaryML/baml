import { type NodeProps } from '@xyflow/react';
import { Sparkles } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { nodeBackground, nodeShadow, selectionRing, stateColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';
import { NodeOutputPreview } from './NodeOutputPreview';

export const LLMNode: ComponentType<NodeProps> = memo(({ data }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  const isRunning = d.executionState === 'running';
  // LLM nodes use a violet accent regardless of state — domain signal first,
  // execution state communicated through the gradient + border tint.
  const base = stateColors[d.executionState] ?? stateColors['not-started'];
  const colors = {
    ...base,
    accent: '#a78bfa',
    border: isHighlighted ? selectionRing.color : 'rgba(167,139,250,0.35)',
  };

  return (
    <>
      <NodeHandles />
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 4,
          padding: '7px 11px 8px 9px',
          borderRadius: 8,
          background: nodeBackground(colors),
          border: `1px solid ${colors.border}`,
          boxShadow: nodeShadow(colors, !!isHighlighted),
          width: '100%',
          height: '100%',
          boxSizing: 'border-box',
          color: colors.text,
          fontFamily: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
          transition: 'box-shadow 120ms ease, border-color 120ms ease',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 4,
              padding: '2px 6px 2px 5px',
              borderRadius: 5,
              fontSize: 9,
              fontWeight: 700,
              letterSpacing: '0.06em',
              background: 'rgba(167,139,250,0.15)',
              color: '#c4b5fd',
              boxShadow: 'inset 0 0 0 1px rgba(167,139,250,0.35)',
            }}
          >
            <Sparkles
              size={9}
              strokeWidth={2.5}
              style={isRunning ? { animation: 'baml-graph-spin 900ms linear infinite' } : undefined}
            />
            LLM
          </span>
          <span
            style={{
              fontSize: 12,
              fontWeight: 600,
              color: '#ddd6fe',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              flex: 1,
              letterSpacing: '-0.005em',
            }}
          >
            {d.label}
          </span>
        </div>
        {d.llmClient && (
          <div
            style={{
              fontSize: 9,
              color: colors.textMuted,
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              whiteSpace: 'nowrap',
              fontVariantNumeric: 'tabular-nums',
              opacity: 0.85,
              paddingLeft: 1,
            }}
          >
            {d.llmClient}
          </div>
        )}
        <NodeOutputPreview
          result={d.result}
          hasResult={d.hasResult}
          images={d.imageOutputs}
          errorMessage={d.errorMessage}
          customRenderers={d.customRenderers}
        />
      </div>
    </>
  );
});

LLMNode.displayName = 'LLMNode';
