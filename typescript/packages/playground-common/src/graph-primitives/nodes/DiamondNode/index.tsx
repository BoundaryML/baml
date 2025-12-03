import { Handle, type NodeProps, Position } from '@xyflow/react';
import { GitBranch } from 'lucide-react';
import { type ComponentType, memo } from 'react';

import type { ReactflowDiamondNode } from '../../../mock-data/types';
// import { ExecutionHistoryDots } from '../subcomponents/ExecutionHistoryDots';

export const DiamondNode: ComponentType<NodeProps<ReactflowDiamondNode>> = memo(
  ({ data, selected }) => {
    // Use direction from node data, fallback to vertical
    const direction = data.direction || 'vertical';
    const isHorizontal = direction === 'horizontal';

    // Execution state visual styling
    const executionState = data.executionState || 'not-started';
    const isExecutionActive = data.isExecutionActive !== false;

    // Define active (bright) and completed (muted) styles
    const stateStyles = {
      'not-started': {
        border: 'border-border',
        bg: 'bg-card',
      },
      'running': {
        active: {
          border: 'border-blue-500',
          bg: 'bg-blue-50 dark:bg-blue-950',
        },
        completed: {
          border: 'border-blue-300 dark:border-blue-700',
          bg: 'bg-blue-50 dark:bg-blue-950 saturate-50',
        },
      },
      'success': {
        active: {
          border: 'border-green-500',
          bg: 'bg-green-50 dark:bg-green-950',
        },
        completed: {
          border: 'border-green-300 dark:border-green-700',
          bg: 'bg-green-50 dark:bg-green-950 saturate-50',
        },
      },
      'error': {
        active: {
          border: 'border-red-500',
          bg: 'bg-red-50 dark:bg-red-950',
        },
        completed: {
          border: 'border-red-300 dark:border-red-700',
          bg: 'bg-red-50 dark:bg-red-950 saturate-50',
        },
      },
      'pending': {
        active: {
          border: 'border-yellow-500 border-dashed',
          bg: 'bg-card',
        },
        completed: {
          border: 'border-yellow-300 dark:border-yellow-700 border-dashed',
          bg: 'bg-card saturate-50',
        },
      },
      'skipped': {
        active: {
          border: 'border-gray-400 border-dashed',
          bg: 'bg-gray-50 dark:bg-gray-900',
        },
        completed: {
          border: 'border-gray-300 dark:border-gray-700 border-dashed',
          bg: 'bg-gray-50 dark:bg-gray-900 saturate-50',
        },
      },
      'cached': {
        active: {
          border: 'border-purple-500',
          bg: 'bg-purple-50 dark:bg-purple-950',
        },
        completed: {
          border: 'border-purple-300 dark:border-purple-700',
          bg: 'bg-purple-50 dark:bg-purple-950 saturate-50',
        },
      },
    };

    // Get the appropriate style based on execution state and whether it's active
    const getStateStyle = () => {
      if (executionState === 'not-started') {
        return stateStyles['not-started'];
      }

      const stateStyle = stateStyles[executionState as keyof typeof stateStyles];
      if (!stateStyle || typeof stateStyle === 'object' && !('active' in stateStyle)) {
        return stateStyles['not-started'];
      }

      return isExecutionActive ? stateStyle.active : stateStyle.completed;
    };

    const currentStyle = getStateStyle();

    return (
      <>
        {/* Hidden handles - positioned at diamond points (corners of rotated square) */}
        {data.targetHandles.map((id) => (
          <Handle
            key={id}
            id={id}
            type="target"
            position={isHorizontal ? Position.Left : Position.Top}
            style={{
              opacity: 0,
              left: isHorizontal ? '0%' : '50%',
              top: isHorizontal ? '50%' : '0%',
              transform: 'translate(-50%, -50%)'
            }}
          />
        ))}
        {data.sourceHandles.map((id) => (
          <Handle
            key={id}
            id={id}
            type="source"
            position={isHorizontal ? Position.Right : Position.Bottom}
            style={{
              opacity: 0,
              left: isHorizontal ? '100%' : '50%',
              top: isHorizontal ? '50%' : 'auto',
              bottom: isHorizontal ? 'auto' : '0%',
              transform: isHorizontal ? 'translate(50%, -50%)' : 'translate(-50%, 50%)'
            }}
          />
        ))}

        {/* Diamond node content with execution state colors */}
        <div className={`
          relative
          flex items-center justify-center
          w-fit h-fit min-w-[110px]
          px-4 py-3
          ${currentStyle.bg} border-2 ${currentStyle.border}
          shadow-sm hover:shadow-md transition-all
          ${selected ? 'font-bold border-primary border-[3px] shadow-lg' : ''}
        `}>
          <div className="flex items-center gap-2 text-card-foreground text-left">
            <span className="inline-flex items-center justify-center rounded bg-amber-100 dark:bg-amber-900 px-1.5 py-1">
              <GitBranch className="w-4 h-4 text-amber-700 dark:text-amber-200" />
            </span>
            <div className="text-xs font-medium leading-tight">
              {data.label || data.id}
            </div>
          </div>

          {/* Execution History Dots - Commented out: UI is messed up for diamond nodes */}
          {/* <ExecutionHistoryDots nodeId={data.id} /> */}
        </div>
      </>
    );
  },
);

DiamondNode.displayName = 'DiamondNode';
