import { Handle, type NodeProps, Position } from '@xyflow/react';
import { type ComponentType, memo } from 'react';

import type { ReactflowHexagonNode} from '../../../mock-data/types';
import { ExecutionHistoryDots } from '../subcomponents/ExecutionHistoryDots';

export const HexagonNode: ComponentType<NodeProps<ReactflowHexagonNode>> = memo(
  ({ data, selected }) => {
    // Use direction from node data, fallback to vertical
    const direction = data.direction || 'vertical';
    const reverseSourceHandles = false;
    const isHorizontal = direction === 'horizontal';
    const targetHandlesFlexDirection: any = isHorizontal ? 'column' : 'row';
    const sourceHandlesFlexDirection: any =
      targetHandlesFlexDirection + (reverseSourceHandles ? '-reverse' : '');

    const handlesBaseClasses = "absolute flex justify-around";
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
              className={`w-2 h-2 border-2 border-[hsl(var(--chart-2))] bg-background ${isHorizontal ? 'top-auto' : 'left-auto'}`}
              id={id}
              key={id}
              position={isHorizontal ? Position.Left : Position.Top}
              type="target"
            />
          ))}
        </div>

        {/* Hexagon/Loop node content with icon */}
        <div className={`
          flex flex-col gap-0 px-3 py-2 pb-1.5 rounded-md
          bg-card border
          shadow-sm hover:shadow-md transition-shadow
          ${selected ? 'ring-2 ring-[hsl(var(--chart-2))] shadow-lg' : ''}
        `}>
          {/* Top row: Icon and Label */}
          <div className="flex items-center gap-2">
            {/* Icon - Loop/Repeat */}
            <div className="flex-shrink-0 w-6 h-6 rounded-full bg-[hsl(var(--chart-2))] flex items-center justify-center">
              <svg className="w-3 h-3 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
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
              className={`w-2 h-2 border-2 border-[hsl(var(--chart-2))] bg-background ${isHorizontal ? 'top-auto' : 'left-auto'}`}
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

HexagonNode.displayName = 'HexagonNode';
