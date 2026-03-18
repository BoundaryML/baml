import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import type { WorkflowNodeData } from '../types';

const stateColors: Record<string, { border: string; bg: string; icon: string }> = {
  'not-started': { border: '#3c3c3c', bg: '#252526', icon: '#4a4a4a' },
  'running':     { border: '#2563eb', bg: '#1e293b', icon: '#2563eb' },
  'success':     { border: '#16a34a', bg: '#14281e', icon: '#16a34a' },
  'error':       { border: '#dc2626', bg: '#2a1515', icon: '#dc2626' },
  'pending':     { border: '#d97706', bg: '#252526', icon: '#d97706' },
  'skipped':     { border: '#6b7280', bg: '#1f1f1f', icon: '#6b7280' },
  'cached':      { border: '#7c3aed', bg: '#1e1528', icon: '#7c3aed' },
};

const StateIcon = ({ state }: { state: string }) => {
  if (state === 'running') {
    return (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3">
        <circle cx="12" cy="12" r="10" opacity="0.25" />
        <path d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" fill="white" opacity="0.75" />
      </svg>
    );
  }
  if (state === 'success') {
    return (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3" strokeLinecap="round">
        <path d="M5 13l4 4L19 7" />
      </svg>
    );
  }
  if (state === 'error') {
    return (
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3" strokeLinecap="round">
        <path d="M6 18L18 6M6 6l12 12" />
      </svg>
    );
  }
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2" strokeLinecap="round">
      <path d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
    </svg>
  );
};

export const BaseNode: ComponentType<NodeProps> = memo(({ data, selected }) => {
  const d = data as WorkflowNodeData;
  const colors = stateColors[d.executionState] ?? stateColors['not-started'];

  return (
    <>
      <Handle type="target" position={Position.Left} style={{ background: '#555' }} />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          borderRadius: 6,
          background: colors.bg,
          border: `2px solid ${colors.border}`,
          boxShadow: selected ? `0 0 0 2px ${colors.border}` : '0 1px 3px rgba(0,0,0,0.3)',
          minWidth: 100,
        }}
      >
        <div
          style={{
            width: 24,
            height: 24,
            borderRadius: '50%',
            background: colors.icon,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
          }}
        >
          <StateIcon state={d.executionState} />
        </div>
        <div style={{ fontSize: 12, fontWeight: 500, color: '#ccc', maxWidth: 140, wordBreak: 'break-word' }}>
          {d.label}
        </div>
      </div>
      <Handle type="source" position={Position.Right} style={{ background: '#555' }} />
    </>
  );
});

BaseNode.displayName = 'BaseNode';
