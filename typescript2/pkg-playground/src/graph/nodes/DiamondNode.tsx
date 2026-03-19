import { Handle, type NodeProps, Position } from '@xyflow/react';
import { GitBranch } from 'lucide-react';
import { type ComponentType, memo } from 'react';
import type { WorkflowNodeData } from '../types';

const stateColors: Record<string, { border: string; bg: string }> = {
  'not-started': { border: '#3c3c3c', bg: '#252526' },
  'running':     { border: '#2563eb', bg: '#1e293b' },
  'success':     { border: '#16a34a', bg: '#14281e' },
  'error':       { border: '#dc2626', bg: '#2a1515' },
  'pending':     { border: '#d97706', bg: '#252526' },
  'skipped':     { border: '#6b7280', bg: '#1f1f1f' },
  'cached':      { border: '#7c3aed', bg: '#1e1528' },
};

export const DiamondNode: ComponentType<NodeProps> = memo(({ data, selected }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected || selected;
  const colors = stateColors[d.executionState] ?? stateColors['not-started'];

  return (
    <>
      <Handle type="target" position={Position.Left} style={{ opacity: 0 }} />
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
      <Handle type="source" position={Position.Right} style={{ opacity: 0 }} />
    </>
  );
});

DiamondNode.displayName = 'DiamondNode';
