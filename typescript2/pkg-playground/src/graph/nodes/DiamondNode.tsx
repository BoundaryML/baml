import { type NodeProps } from '@xyflow/react';
import { GitBranch } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { nodeBackground, nodeShadow, selectionRing, stateColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

export const DiamondNode: ComponentType<NodeProps> = memo(({ data }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected;
  // Conditionals are visually distinct via amber accent regardless of state.
  const base = stateColors[d.executionState] ?? stateColors['not-started'];
  const colors = {
    ...base,
    accent: '#f59e0b',
    border: isHighlighted ? selectionRing.color : 'rgba(245,158,11,0.35)',
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
        <span
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            width: 20,
            height: 20,
            borderRadius: 6,
            background: 'rgba(245,158,11,0.15)',
            boxShadow: 'inset 0 0 0 1px rgba(245,158,11,0.35)',
          }}
        >
          <GitBranch size={12} color="#fbbf24" style={{ transform: 'rotate(180deg)' }} />
        </span>
        <div
          style={{
            fontSize: 12,
            fontWeight: 500,
            color: colors.text,
            lineHeight: 1.3,
            letterSpacing: '-0.005em',
          }}
        >
          {d.label}
        </div>
      </div>
    </>
  );
});

DiamondNode.displayName = 'DiamondNode';
