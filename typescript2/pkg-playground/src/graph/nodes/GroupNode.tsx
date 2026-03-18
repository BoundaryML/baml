import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import type { WorkflowNodeData } from '../types';

const stateBorderColors: Record<string, string> = {
  'not-started': '#3c3c3c',
  'running': '#2563eb',
  'success': '#16a34a',
  'error': '#dc2626',
  'pending': '#d97706',
  'skipped': '#6b7280',
  'cached': '#7c3aed',
};

export const GroupNode: ComponentType<NodeProps> = memo(({ data, id }) => {
  const d = data as WorkflowNodeData;
  const borderColor = stateBorderColors[d.executionState] ?? stateBorderColors['not-started'];

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        position: 'relative',
        pointerEvents: 'none',
        borderRadius: 8,
        border: `1px dashed ${borderColor}`,
        background: 'rgba(37,37,38,0.5)',
      }}
    >
      <Handle type="target" position={Position.Left} style={{ opacity: 0, pointerEvents: 'auto' }} />

      {/* Group label */}
      <div
        style={{
          position: 'absolute',
          top: 0,
          left: '50%',
          transform: 'translate(-50%, -50%)',
          zIndex: 1000,
          pointerEvents: 'auto',
          whiteSpace: 'nowrap',
          padding: '4px 12px',
          borderRadius: 6,
          fontWeight: 600,
          fontSize: 12,
          color: '#ccc',
          background: '#2d2d2d',
          border: `1px solid ${borderColor}`,
          boxShadow: '0 1px 3px rgba(0,0,0,0.3)',
        }}
      >
        {d.label || id}
        {(d.iterationCount ?? 0) > 0 && (
          <span
            style={{
              marginLeft: 6,
              padding: '1px 6px',
              borderRadius: 4,
              background: 'rgba(37,99,235,0.2)',
              color: '#60a5fa',
              fontSize: 10,
              fontWeight: 500,
            }}
          >
            {d.iterationCount}
          </span>
        )}
      </div>

      <Handle type="source" position={Position.Right} style={{ opacity: 0, pointerEvents: 'auto' }} />
    </div>
  );
});

GroupNode.displayName = 'GroupNode';
