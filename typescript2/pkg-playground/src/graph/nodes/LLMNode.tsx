import { type NodeProps } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import { stateColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

export const LLMNode: ComponentType<NodeProps> = memo(({ data, selected }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected || selected;
  const colors = stateColors[d.executionState] ?? stateColors['not-started'];

  return (
    <>
      <NodeHandles />
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 4,
          padding: '8px 12px',
          borderRadius: 6,
          background: colors.bg,
          border: `2px solid ${isHighlighted ? '#4fc3f7' : colors.border}`,
          boxShadow: isHighlighted ? `0 0 0 3px #4fc3f7, 0 0 12px rgba(79,195,247,0.4)` : '0 1px 3px rgba(0,0,0,0.3)',
          minWidth: 160,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
          <span
            style={{
              padding: '2px 6px',
              borderRadius: 4,
              fontSize: 8,
              fontWeight: 700,
              background: colors.accent,
              color: 'white',
              letterSpacing: '0.05em',
            }}
          >
            LLM
          </span>
          <span style={{ fontSize: 12, fontWeight: 600, color: '#a78bfa', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', flex: 1 }}>
            {d.label}
          </span>
        </div>
        {d.llmClient && (
          <div style={{ fontSize: 9, color: '#858585', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {d.llmClient}
          </div>
        )}
      </div>
    </>
  );
});

LLMNode.displayName = 'LLMNode';
