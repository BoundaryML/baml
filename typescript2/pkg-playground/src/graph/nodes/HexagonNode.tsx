import { type NodeProps } from '@xyflow/react';
import { Repeat } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { nodeBackground, nodeShadow, selectionRing, stateColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

export const HexagonNode: ComponentType<NodeProps> = memo(({ data }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  // Loops use the cyan accent regardless of state.
  const base = stateColors[d.executionState] ?? stateColors['not-started'];
  const colors = {
    ...base,
    accent: '#0ea5e9',
    border: isHighlighted ? selectionRing.color : 'rgba(14,165,233,0.35)',
  };

  return (
    <>
      <NodeHandles />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '7px 11px 7px 9px',
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
        <div
          style={{
            width: 20,
            height: 20,
            borderRadius: 6,
            background: 'rgba(14,165,233,0.15)',
            boxShadow: 'inset 0 0 0 1px rgba(14,165,233,0.35)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
          }}
        >
          <Repeat size={12} color="#7dd3fc" />
        </div>
        <div
          style={{
            fontSize: 12,
            fontWeight: 500,
            color: colors.text,
            flex: 1,
            minWidth: 0,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            letterSpacing: '-0.005em',
            lineHeight: 1.3,
          }}
        >
          {d.label}
        </div>
      </div>
    </>
  );
});

HexagonNode.displayName = 'HexagonNode';
