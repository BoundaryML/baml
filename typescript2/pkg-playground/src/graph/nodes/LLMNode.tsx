import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import type { WorkflowNodeData } from '../types';

const stateColors: Record<string, { border: string; bg: string; badge: string }> = {
  'not-started': { border: '#3c3c3c', bg: '#252526', badge: '#4a4a4a' },
  'running':     { border: '#2563eb', bg: '#1e293b', badge: '#2563eb' },
  'success':     { border: '#16a34a', bg: '#14281e', badge: '#16a34a' },
  'error':       { border: '#dc2626', bg: '#2a1515', badge: '#dc2626' },
  'pending':     { border: '#d97706', bg: '#252526', badge: '#d97706' },
  'skipped':     { border: '#6b7280', bg: '#1f1f1f', badge: '#6b7280' },
  'cached':      { border: '#7c3aed', bg: '#1e1528', badge: '#7c3aed' },
};

export const LLMNode: ComponentType<NodeProps> = memo(({ data, selected }) => {
  const d = data as WorkflowNodeData;
  const colors = stateColors[d.executionState] ?? stateColors['not-started'];

  return (
    <>
      <Handle type="target" position={Position.Left} style={{ background: '#7c3aed' }} />
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 4,
          padding: '8px 12px',
          borderRadius: 6,
          background: colors.bg,
          border: `2px solid ${colors.border}`,
          boxShadow: selected ? `0 0 0 2px ${colors.border}` : '0 1px 3px rgba(0,0,0,0.3)',
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
              background: colors.badge,
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
      <Handle type="source" position={Position.Right} style={{ background: '#7c3aed' }} />
    </>
  );
});

LLMNode.displayName = 'LLMNode';
