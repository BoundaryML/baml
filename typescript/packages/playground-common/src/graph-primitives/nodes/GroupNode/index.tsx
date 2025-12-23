import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';
import { Loader as Spinner } from '@baml/ui/custom/loader';
import { RefreshCw } from 'lucide-react';

export const GroupNode: ComponentType<NodeProps> = memo(({ data, id }) => {
  // Use direction from node data, fallback to vertical
  const direction = (data as any).direction || 'vertical';
  const isHorizontal = direction === 'horizontal';
  const targetHandlesFlexDirection: any = isHorizontal ? 'column' : 'row';
  const sourceHandlesFlexDirection: any = targetHandlesFlexDirection;

  // Execution state styling - very subtle border colors
  const executionState = (data as any).executionState || 'not-started';
  const iteration = (data as any).iteration ?? 0;
  const labelStateStyles: Record<string, string> = {
    'not-started': 'bg-muted border border-border text-muted-foreground',
    'running': 'bg-muted border border-blue-300 dark:border-blue-500 text-muted-foreground',
    'success': 'bg-muted dark:bg-green-700/70 dark:text-white border border-green-300 dark:border-green-500 text-muted-foreground',
    'error': 'bg-muted border border-red-300 dark:border-red-700 text-muted-foreground',
    'pending': 'bg-muted border border-yellow-300 dark:border-yellow-700 text-muted-foreground',
    'skipped': 'bg-muted border border-border text-muted-foreground opacity-60',
    'cached': 'bg-muted border border-purple-300 dark:border-purple-700 text-muted-foreground',
  };
  const labelStyle = labelStateStyles[executionState] || labelStateStyles['not-started'];

  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        position: 'relative',
        pointerEvents: 'none', // Allow clicking through to child nodes
      }}
    >
      {/* Target handles (top) */}
      <div
        className={`handles handles-${direction} targets`}
        style={{
          position: 'absolute',
          display: 'flex',
          justifyContent: 'space-around',
          flexDirection: targetHandlesFlexDirection,
          width: isHorizontal ? '10px' : '100%',
          height: isHorizontal ? '100%' : '10px',
          top: isHorizontal ? '0px' : '-4px',
          left: isHorizontal ? '-4px' : '0px',
          pointerEvents: 'auto',
        }}
      >
        {(data as any).targetHandles?.map((handleId: string) => (
          <Handle
            className={`handle handle-${direction} w-2 h-2 border-2 border-accent bg-background`}
            id={handleId}
            key={handleId}
            position={isHorizontal ? Position.Left : Position.Top}
            style={{
              position: 'relative',
              top: 'auto',
              left: 'auto',
              transform: 'none',
            }}
            type="target"
          />
        ))}
      </div>

      {/* Running spinner - top left */}
      {executionState === 'running' && (
        <div className="absolute top-2 left-2 z-[1001] pointer-events-none">
          <Spinner className="w-4 h-4 text-blue-500" />
        </div>
      )}

      {/* Group label */}
      <div
        className={`absolute -top-0 left-1/2 -translate-x-1/2 z-[1000] pointer-events-auto whitespace-nowrap px-3 py-1.5 rounded-md font-semibold text-sm shadow-sm ${labelStyle}`}
      >
        <span className="flex items-center gap-1.5">
          {(data as any).label || id}
          {iteration > 0 && (
            <span className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-600 dark:text-blue-400 text-xs font-medium">
              <RefreshCw className="w-3 h-3" />
              {iteration + 1}
            </span>
          )}
        </span>
      </div>

      {/* Source handles (bottom) */}
      <div
        className={`handles handles-${direction} sources`}
        style={{
          position: 'absolute',
          display: 'flex',
          justifyContent: 'space-around',
          flexDirection: sourceHandlesFlexDirection,
          width: isHorizontal ? '10px' : '100%',
          height: isHorizontal ? '100%' : '10px',
          top: isHorizontal ? '0px' : 'auto',
          bottom: isHorizontal ? 'auto' : '-2px',
          right: isHorizontal ? '-2px' : 'auto',
          left: isHorizontal ? 'auto' : '0px',
          pointerEvents: 'auto',
        }}
      >
        {(data as any).sourceHandles?.map((handleId: string) => (
          <Handle
            className={`handle handle-${direction} w-2 h-2 border-2 border-emerald-500 bg-white dark:bg-gray-800`}
            id={handleId}
            key={handleId}
            position={isHorizontal ? Position.Right : Position.Bottom}
            style={{
              position: 'relative',
              top: 'auto',
              left: 'auto',
              transform: 'none',
            }}
            type="source"
          />
        ))}
      </div>
    </div>
  );
});

GroupNode.displayName = 'GroupNode';
