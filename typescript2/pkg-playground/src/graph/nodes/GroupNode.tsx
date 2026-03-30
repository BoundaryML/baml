import { type NodeProps } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import { stateBorderColors } from '../constants';
import type { WorkflowNodeData } from '../types';
import { NodeHandles } from './NodeHandles';

export const GroupNode: ComponentType<NodeProps> = memo(({ data, id, selected }) => {
  const d = data as WorkflowNodeData;
  const isHighlighted = d.selected || selected;
  const borderColor = isHighlighted
    ? '#4fc3f7'
    : (stateBorderColors[d.executionState] ?? stateBorderColors['not-started']);

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
        boxShadow: isHighlighted ? '0 0 8px rgba(79,195,247,0.3)' : undefined,
        transition: 'border-color 0.15s, box-shadow 0.15s',
      }}
    >
      <NodeHandles />

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
          color: isHighlighted ? '#fff' : '#ccc',
          background: isHighlighted ? '#1a3a4a' : '#2d2d2d',
          border: `1px solid ${borderColor}`,
          boxShadow: isHighlighted
            ? '0 0 0 2px #4fc3f7, 0 0 10px rgba(79,195,247,0.4)'
            : '0 1px 3px rgba(0,0,0,0.3)',
          transition: 'all 0.15s',
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

    </div>
  );
});

GroupNode.displayName = 'GroupNode';
