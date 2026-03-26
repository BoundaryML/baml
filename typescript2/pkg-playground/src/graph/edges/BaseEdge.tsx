import { BaseEdge as RFBaseEdge, type EdgeProps, type Edge, getSmoothStepPath } from '@xyflow/react';
import { memo } from 'react';
import { getMarkerColors } from './Marker';
import type { WorkflowEdgeData, EdgePathData } from '../types';

/**
 * Build an SVG path string from ELK edge section points
 * with rounded corners at bend points.
 */
function buildElkPath(points: Array<{ x: number; y: number }>, radius = 8): string {
  if (points.length < 2) return '';
  if (points.length === 2) {
    return `M ${points[0].x} ${points[0].y} L ${points[1].x} ${points[1].y}`;
  }

  let d = `M ${points[0].x} ${points[0].y}`;

  for (let i = 1; i < points.length - 1; i++) {
    const prev = points[i - 1];
    const curr = points[i];
    const next = points[i + 1];

    const dPrev = Math.hypot(curr.x - prev.x, curr.y - prev.y);
    const dNext = Math.hypot(next.x - curr.x, next.y - curr.y);
    const r = Math.min(radius, dPrev / 2, dNext / 2);

    const startX = curr.x - (r * (curr.x - prev.x)) / dPrev;
    const startY = curr.y - (r * (curr.y - prev.y)) / dPrev;
    const endX = curr.x + (r * (next.x - curr.x)) / dNext;
    const endY = curr.y + (r * (next.y - curr.y)) / dNext;

    d += ` L ${startX} ${startY}`;
    d += ` Q ${curr.x} ${curr.y} ${endX} ${endY}`;
  }

  const last = points[points.length - 1];
  d += ` L ${last.x} ${last.y}`;

  return d;
}

export const BaseEdge = memo<EdgeProps<Edge<WorkflowEdgeData>>>(
  ({
    id,
    selected,
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    style,
    markerStart,
    interactionWidth,
    data,
  }) => {
    const colors = getMarkerColors();
    const edgeColor = data?.color ?? colors.base;
    const pathData = data?.pathData as EdgePathData | undefined;

    let edgePath: string;

    if (pathData?.points && pathData.points.length >= 2) {
      edgePath = buildElkPath(pathData.points);
    } else {
      [edgePath] = getSmoothStepPath({
        sourceX,
        sourceY,
        targetX,
        targetY,
        sourcePosition,
        targetPosition,
        borderRadius: 12,
      });
    }

    return (
      <RFBaseEdge
        id={id}
        interactionWidth={interactionWidth}
        markerEnd={`url(#${edgeColor.replace('#', '')})`}
        markerStart={markerStart}
        path={edgePath}
        style={{
          stroke: edgeColor,
          opacity: selected ? 1 : 0.6,
          strokeWidth: selected ? 2 : 1.5,
          ...style,
        }}
      />
    );
  },
);

BaseEdge.displayName = 'BaseEdge';
