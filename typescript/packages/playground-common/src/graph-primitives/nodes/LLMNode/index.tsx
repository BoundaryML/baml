import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';

import type { ReactflowBaseNode } from '../../../mock-data/types';
import { ExecutionHistoryDots } from '../subcomponents/ExecutionHistoryDots';

export const LLMNode: ComponentType<NodeProps<ReactflowBaseNode>> = memo(
  ({ data, selected }) => {
    // Use direction from node data, fallback to vertical
    const direction = data.direction || 'vertical';
    const isHorizontal = direction === 'horizontal';
    const targetHandlesFlexDirection: 'row' | 'column' = isHorizontal ? 'column' : 'row';
    const sourceHandlesFlexDirection: 'row' | 'column' = isHorizontal ? 'column' : 'row';

    // Execution state visual styling
    const executionState = data.executionState || 'not-started';
    const isExecutionActive = data.isExecutionActive !== false;

    // Get LLM-specific data
    const llmClient = data.llmClient || 'Unknown';
    const outputs = data.outputs;
    const error = data.error;

    // Define active (bright) and completed (muted) styles
    const stateStyles = {
      'not-started': {
        border: 'border-border',
        bg: 'bg-card',
        badge: 'bg-muted',
        badgeText: 'text-muted-foreground',
      },
      'running': {
        active: {
          border: 'border-blue-500',
          bg: 'bg-blue-50 dark:bg-blue-950',
          badge: 'bg-blue-500 animate-pulse',
          badgeText: 'text-white',
        },
        completed: {
          border: 'border-blue-300 dark:border-blue-700',
          bg: 'bg-blue-50 dark:bg-blue-950 saturate-50',
          badge: 'bg-blue-300 dark:bg-blue-700',
          badgeText: 'text-white',
        },
      },
      'success': {
        active: {
          border: 'border-green-500',
          bg: 'bg-green-50 dark:bg-green-950',
          badge: 'bg-green-500',
          badgeText: 'text-white',
        },
        completed: {
          border: 'border-green-300 dark:border-green-700',
          bg: 'bg-green-50 dark:bg-green-950 saturate-50',
          badge: 'bg-green-300 dark:bg-green-700',
          badgeText: 'text-white',
        },
      },
      'error': {
        active: {
          border: 'border-red-500',
          bg: 'bg-red-50 dark:bg-red-950',
          badge: 'bg-red-500',
          badgeText: 'text-white',
        },
        completed: {
          border: 'border-red-300 dark:border-red-700',
          bg: 'bg-red-50 dark:bg-red-950 saturate-50',
          badge: 'bg-red-300 dark:bg-red-700',
          badgeText: 'text-white',
        },
      },
      'pending': {
        active: {
          border: 'border-yellow-500 border-dashed',
          bg: 'bg-card',
          badge: 'bg-yellow-500',
          badgeText: 'text-white',
        },
        completed: {
          border: 'border-yellow-300 dark:border-yellow-700 border-dashed',
          bg: 'bg-card saturate-50',
          badge: 'bg-yellow-300 dark:bg-yellow-700',
          badgeText: 'text-white',
        },
      },
      'skipped': {
        active: {
          border: 'border-gray-400 border-dashed',
          bg: 'bg-gray-50 dark:bg-gray-900',
          badge: 'bg-gray-400',
          badgeText: 'text-white',
        },
        completed: {
          border: 'border-gray-300 dark:border-gray-700 border-dashed',
          bg: 'bg-gray-50 dark:bg-gray-900 saturate-50',
          badge: 'bg-gray-300 dark:bg-gray-700',
          badgeText: 'text-white',
        },
      },
      'cached': {
        active: {
          border: 'border-purple-500',
          bg: 'bg-purple-50 dark:bg-purple-950',
          badge: 'bg-purple-500',
          badgeText: 'text-white',
        },
        completed: {
          border: 'border-purple-300 dark:border-purple-700',
          bg: 'bg-purple-50 dark:bg-purple-950 saturate-50',
          badge: 'bg-purple-300 dark:bg-purple-700',
          badgeText: 'text-white',
        },
      },
    };

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

    // Format output preview (truncate to 3 lines)
    const getOutputPreview = () => {
      if (error) {
        const errorMsg = typeof error === 'string' ? error : error?.message || 'Error occurred';
        return errorMsg.substring(0, 150); // Truncate error message
      }
      if (!outputs) {
        return '<no output yet>';
      }
      const outputText = typeof outputs === 'string' ? outputs : JSON.stringify(outputs, null, 2);
      // Truncate to ~150 characters (approx 3 lines of small text)
      return outputText.length > 150 ? outputText.substring(0, 147) + '...' : outputText;
    };

    return (
      <>
        <div
          // className={`${handlesBaseClasses} ${handlesDirectionClasses} ${targetsPositionClasses}`}
          className='flex flex-col gap-0.5 px-3 py-2 rounded-md min-w-[180px] h-fit'
          style={{
            flexDirection: targetHandlesFlexDirection,
          }}
        >
          {data.targetHandles.map((id) => (
            <Handle
              className={`w-2 h-2 border-2 border-purple-500 bg-background ${isHorizontal ? 'top-auto' : 'left-auto'}`}
              id={id}
              key={id}
              position={isHorizontal ? Position.Left : Position.Top}
              type="target"
            />
          ))}
        </div>

        {/* Node content with LLM badge and output preview */}
        <div className={`
          flex flex-col gap-0.5 px-3 py-2 pb-1.5 rounded-md min-w-[180px] max-h-[220px] nowheel
          ${currentStyle.bg} border-2 ${currentStyle.border}
          shadow-sm hover:shadow-md transition-all overflow-visible
          ${selected ? 'ring-2 ring-primary shadow-lg' : ''}
        `}>
          {/* Header with LLM badge */}
          <div className="flex items-center gap-1.5">
            <div className={`px-1 py-0.5 rounded text-[8px] font-bold ${currentStyle.badge} ${currentStyle.badgeText}`}>
              LLM
            </div>
            <div className="text-xs font-semibold text-purple-600 dark:text-purple-400 truncate flex-1">
              {data.label || data.id}
            </div>
          </div>

          {/* Client name */}
          <div className="text-[9px] text-muted-foreground truncate">
            {llmClient}
          </div>

          {/* Output preview - 1 line truncated */}
          <pre className={`text-[8px] font-mono min-h-[40px] break-words whitespace-pre-wrap max-h-[60px] overflow-y-auto truncate ${error ? 'text-red-600 dark:text-red-400' : 'text-muted-foreground'
            }`}>
            {getOutputPreview()}
          </pre>

          {/* Execution History Dots */}
          <ExecutionHistoryDots nodeId={data.id} />
        </div>

        <div
          // className={`${handlesBaseClasses} ${handlesDirectionClasses} ${sourcesPositionClasses}`}
          style={{
            flexDirection: sourceHandlesFlexDirection,
          }}
        >
          {data.sourceHandles.map((id) => (
            <Handle
              className={`w-2 h-2 border-2 border-purple-500 bg-background ${isHorizontal ? 'top-auto' : 'left-auto'}`}
              id={id}
              key={id}
              position={isHorizontal ? Position.Right : Position.Bottom}
              type="source"
            />
          ))}
        </div>
      </>
    );
  },
);

LLMNode.displayName = 'LLMNode';
