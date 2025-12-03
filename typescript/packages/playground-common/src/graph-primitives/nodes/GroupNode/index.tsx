import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';

export const GroupNode: ComponentType<NodeProps> = memo(({ data, id }) => {
  // Use direction from node data, fallback to vertical
  const direction = (data as any).direction || 'vertical';
  const isHorizontal = direction === 'horizontal';
  const targetHandlesFlexDirection: any = isHorizontal ? 'column' : 'row';
  const sourceHandlesFlexDirection: any = targetHandlesFlexDirection;

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

      {/* Group label */}
      <div
        className="absolute -top-0 left-1/2 -translate-x-1/2 z-[1000] pointer-events-auto whitespace-nowrap px-3 py-1.5 rounded-md bg-muted border text-muted-foreground font-semibold text-sm shadow-sm"
      >
        {(data as any).label || id}
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
