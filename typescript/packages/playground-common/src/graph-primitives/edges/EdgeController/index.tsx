import { uuid } from '@del-wang/utils';
import { EdgeLabelRenderer, type Position } from '@xyflow/react';
import { useMemo } from 'react';

import { getLineCenter } from '../../../features/graph/layout/edge/edge';
import type { ControlPoint } from '../../../features/graph/layout/edge/point';
import { flowStore } from '../../../states/reactflow';

import { getEdgeContext, SmartEdge } from './smart-edge';
import { useEdgeDraggable } from './useEdgeDraggable';

export interface EdgeControllersParams {
  id: string;
  points: ControlPoint[];
  sourcePosition: Position;
  targetPosition: Position;
  offset: number;
  handlerWidth: number;
  handlerThickness: number;
}

export const EdgeControllers = (props: EdgeControllersParams) => {
  const { points } = props;

  const edges = useMemo(() => {
    const smartEdges: SmartEdge[] = [];
    const edgeContext = getEdgeContext(props);
    for (let i = 0; i < points.length - 1; i++) {
      smartEdges.push(
        new SmartEdge({
          idx: i,
          start: points[i]!,
          end: points[i + 1]!,
          ctx: edgeContext,
        }),
      );
    }
    smartEdges.forEach((e, idx) => {
      e.previous = smartEdges[idx - 2];
      e.next = smartEdges[idx + 2];
    });
    return smartEdges;
  }, [points]);

  return (
    <>
      {edges.map((edge) => {
        return edge.canDrag && <EdgeController edge={edge} key={uuid()} />; // use unique key to force rebuild EdgeController
      })}
    </>
  );
};

export const EdgeController = ({ edge }: { edge: SmartEdge }) => {
  const { start, end, onDragging } = edge;
  const { handlerWidth, handlerThickness } = edge.ctx;
  const center = getLineCenter(start, end);
  const isHorizontal = start.y === end.y;

  const { dragRef } = useEdgeDraggable({
    edge,
    onDragging(dragId, dragFrom, position, delta) {
      const oldFlowPosition = flowStore.value.screenToFlowPosition({
        x: position.x - delta.x,
        y: position.y - delta.y,
      });
      const newFlowPosition = flowStore.value.screenToFlowPosition(position);
      const flowDelta = {
        x: newFlowPosition.x - oldFlowPosition.x,
        y: newFlowPosition.y - oldFlowPosition.y,
      };
      const newStart = { ...start };
      const newEnd = { ...end };
      if (isHorizontal) {
        newStart.y += flowDelta.y;
        newEnd.y += flowDelta.y;
      } else {
        newStart.x += flowDelta.x;
        newEnd.x += flowDelta.x;
      }
      onDragging({
        dragId,
        dragFrom,
        from: { start, end },
        to: { start: newStart, end: newEnd },
      });
    },
  });

  return (
    <EdgeLabelRenderer>
      <div
        className="nodrag nopan"
        ref={dragRef}
        style={{
          cursor: isHorizontal ? 'row-resize' : 'col-resize',
          position: 'absolute',
          transform: `translate(-50%, -50%) translate(${center.x}px,${center.y}px)`,
          width: isHorizontal ? `${handlerWidth}px` : `${handlerThickness}px`,
          height: !isHorizontal ? `${handlerWidth}px` : `${handlerThickness}px`,
          borderRadius: '2px',
          background: '#3579f6',
          border: '1px solid #fff',
          pointerEvents: 'all',
        }}
      />
    </EdgeLabelRenderer>
  );
};
