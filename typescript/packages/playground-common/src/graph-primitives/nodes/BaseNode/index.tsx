import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';

import type { ReactflowBaseNode } from '../../../mock-data/types';
import { ExecutionHistoryDots } from '../subcomponents/ExecutionHistoryDots';

export const BaseNode: ComponentType<NodeProps<ReactflowBaseNode>> = memo(
  ({ data, selected }) => {
    // Use direction from node data, fallback to vertical
    const direction = data.direction || 'vertical';
    const reverseSourceHandles = false;
    const isHorizontal = direction === 'horizontal';
    const targetHandlesFlexDirection: 'row' | 'column' = isHorizontal ? 'column' : 'row';
    const sourceHandlesFlexDirection: 'row' | 'column' | 'row-reverse' | 'column-reverse' =
      (targetHandlesFlexDirection + (reverseSourceHandles ? '-reverse' : '')) as 'row' | 'column' | 'row-reverse' | 'column-reverse';

    // Execution state visual styling
    const executionState = data.executionState || 'not-started';
    const isExecutionActive = data.isExecutionActive !== false; // Default to true if not set

    // Define active (bright) and completed (muted) styles
    const stateStyles = {
      'not-started': {
        border: 'border-border',
        bg: 'bg-card',
        icon: 'bg-muted',
        iconText: 'text-muted-foreground',
      },
      'running': {
        active: {
          border: 'border-blue-500',
          bg: 'bg-blue-50 dark:bg-blue-950',
          icon: 'bg-blue-500 animate-pulse',
          iconText: 'text-white',
        },
        completed: {
          border: 'border-blue-300 dark:border-blue-700',
          bg: 'bg-blue-50 dark:bg-blue-950 saturate-50',
          icon: 'bg-blue-300 dark:bg-blue-700',
          iconText: 'text-white',
        },
      },
      'success': {
        active: {
          border: 'border-green-500',
          bg: 'bg-green-50 dark:bg-green-950',
          icon: 'bg-green-500',
          iconText: 'text-white',
        },
        completed: {
          border: 'border-green-300 dark:border-green-700',
          bg: 'bg-green-50 dark:bg-green-950 saturate-50',
          icon: 'bg-green-300 dark:bg-green-700',
          iconText: 'text-white',
        },
      },
      'error': {
        active: {
          border: 'border-red-500',
          bg: 'bg-red-50 dark:bg-red-950',
          icon: 'bg-red-500',
          iconText: 'text-white',
        },
        completed: {
          border: 'border-red-300 dark:border-red-700',
          bg: 'bg-red-50 dark:bg-red-950 saturate-50',
          icon: 'bg-red-300 dark:bg-red-700',
          iconText: 'text-white',
        },
      },
      'pending': {
        active: {
          border: 'border-yellow-500 border-dashed',
          bg: 'bg-card',
          icon: 'bg-yellow-500',
          iconText: 'text-white',
        },
        completed: {
          border: 'border-yellow-300 dark:border-yellow-700 border-dashed',
          bg: 'bg-card saturate-50',
          icon: 'bg-yellow-300 dark:bg-yellow-700',
          iconText: 'text-white',
        },
      },
      'skipped': {
        active: {
          border: 'border-gray-400 border-dashed',
          bg: 'bg-gray-50 dark:bg-gray-900',
          icon: 'bg-gray-400',
          iconText: 'text-white',
        },
        completed: {
          border: 'border-gray-300 dark:border-gray-700 border-dashed',
          bg: 'bg-gray-50 dark:bg-gray-900 saturate-50',
          icon: 'bg-gray-300 dark:bg-gray-700',
          iconText: 'text-white',
        },
      },
      'cached': {
        active: {
          border: 'border-purple-500',
          bg: 'bg-purple-50 dark:bg-purple-950',
          icon: 'bg-purple-500',
          iconText: 'text-white',
        },
        completed: {
          border: 'border-purple-300 dark:border-purple-700',
          bg: 'bg-purple-50 dark:bg-purple-950 saturate-50',
          icon: 'bg-purple-300 dark:bg-purple-700',
          iconText: 'text-white',
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

    const handlesBaseClasses = "flex justify-around";
    const handlesDirectionClasses = isHorizontal ? "w-[10px] h-full" : "w-full h-[10px]";
    const targetsPositionClasses = isHorizontal ? "top-0 -left-1" : "-top-1 left-0";
    const sourcesPositionClasses = isHorizontal ? "top-0 -right-[2px]" : "-bottom-[2px] left-0";

    return (
      <>
        <div
          className={`${handlesBaseClasses} ${handlesDirectionClasses} ${targetsPositionClasses}`}
          style={{
            flexDirection: targetHandlesFlexDirection,
          }}
        >
          {data.targetHandles.map((id) => (
            <Handle
              className={`w-2 h-2 border-2 border-primary bg-background ${isHorizontal ? 'top-auto' : 'left-auto'}`}
              id={id}
              key={id}
              position={isHorizontal ? Position.Left : Position.Top}
              type="target"
            />
          ))}
        </div>

        {/* Node content with icon */}
        <div className={`
          flex flex-col gap-0 px-3 py-2 pb-1.5 rounded-md
          ${currentStyle.bg} border-2 ${currentStyle.border}
          shadow-sm hover:shadow-md transition-all
          ${selected ? 'ring-2 ring-primary shadow-lg' : ''}
        `}>
          {/* Top row: Icon and Label */}
          <div className="flex items-center gap-2">
            {/* Icon */}
            <div className={`flex-shrink-0 w-6 h-6 rounded-full ${currentStyle.icon} flex items-center justify-center`}>
              {executionState === 'running' ? (
                // Spinner for running state
                <svg className={`w-3 h-3 ${currentStyle.iconText} animate-spin`} fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                </svg>
              ) : executionState === 'success' ? (
                // Checkmark for success
                <svg className={`w-3 h-3 ${currentStyle.iconText}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                </svg>
              ) : executionState === 'error' ? (
                // X for error
                <svg className={`w-3 h-3 ${currentStyle.iconText}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              ) : (
                // Default icon
                <svg className={`w-3 h-3 ${currentStyle.iconText}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
                </svg>
              )}
            </div>
            {/* Label */}
            <div className="text-xs font-medium text-card-foreground break-words max-w-[140px]">{data.label || data.id}</div>
          </div>

          {/* Execution History Dots */}
          <ExecutionHistoryDots nodeId={data.id} />
        </div>

        <div
          className={`${handlesBaseClasses} ${handlesDirectionClasses} ${sourcesPositionClasses}`}
          style={{
            flexDirection: sourceHandlesFlexDirection,
          }}
        >
          {data.sourceHandles.map((id) => (
            <Handle
              className={`w-2 h-2 border-2 border-primary bg-background ${isHorizontal ? 'top-auto' : 'left-auto'}`}
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

BaseNode.displayName = 'BaseNode';
