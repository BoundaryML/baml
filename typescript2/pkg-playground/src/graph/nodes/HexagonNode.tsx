import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import type { WorkflowNodeData } from '../types';

export const HexagonNode: ComponentType<NodeProps> = memo(({ data, selected }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected || selected;
  const borderColor = isHighlighted ? '#4fc3f7' : '#3c3c3c';

  return (
    <>
      <Handle type="target" position={Position.Left} style={{ background: '#0ea5e9' }} />
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          borderRadius: 6,
          background: '#252526',
          border: `2px solid ${borderColor}`,
          boxShadow: isHighlighted ? `0 0 0 3px #4fc3f7, 0 0 12px rgba(79,195,247,0.4)` : '0 1px 3px rgba(0,0,0,0.3)',
        }}
      >
        <div
          style={{
            width: 24,
            height: 24,
            borderRadius: '50%',
            background: '#0ea5e9',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
          }}
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2" strokeLinecap="round">
            <path d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </div>
        <div style={{ fontSize: 12, fontWeight: 500, color: '#ccc', maxWidth: 140, wordBreak: 'break-word' }}>
          {d.label}
        </div>
      </div>
      <Handle type="source" position={Position.Right} style={{ background: '#0ea5e9' }} />
    </>
  );
});

HexagonNode.displayName = 'HexagonNode';
