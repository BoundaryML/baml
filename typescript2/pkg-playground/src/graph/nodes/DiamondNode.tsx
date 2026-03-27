import { type NodeProps } from '@xyflow/react';
import { GitBranch } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import { stateColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

export const DiamondNode: ComponentType<NodeProps> = memo(({ data, selected }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected || selected;
  const colors = stateColors[d.executionState] ?? stateColors['not-started'];

  return (
    <>
      <NodeHandles />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '10px 16px',
          borderRadius: 6,
          background: colors.bg,
          border: `2px solid ${isHighlighted ? '#4fc3f7' : colors.border}`,
          boxShadow: isHighlighted ? `0 0 0 3px #4fc3f7, 0 0 12px rgba(79,195,247,0.4)` : '0 1px 3px rgba(0,0,0,0.3)',
          minWidth: 110,
        }}
      >
        <span
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            borderRadius: 4,
            background: '#78350f',
            padding: '4px 6px',
          }}
        >
          <GitBranch size={16} color="#fbbf24" style={{ transform: 'rotate(180deg)' }} />
        </span>
        <div style={{ fontSize: 12, fontWeight: 500, color: '#ccc', lineHeight: 1.3 }}>
          {d.label}
        </div>
      </div>
    </>
  );
});

DiamondNode.displayName = 'DiamondNode';
