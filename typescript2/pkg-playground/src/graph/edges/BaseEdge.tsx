import { BaseEdge as RFBaseEdge, type EdgeProps, type Edge, getSmoothStepPath } from '@xyflow/react';
import { memo } from 'react';
import { getMarkerColors } from './Marker';
import type { WorkflowEdgeData } from '../types';

/**
 * Build an SVG path from an orthogonal polyline (axis-aligned segments)
 * with rounded corners at each turn.
 *
 * Each interior point is a corner: we cut the radius off both adjacent
 * segments and join them with a quadratic curve through the original
 * corner. Diagonal segments fall back to a straight L (no rounding).
 *
 * Ported from the typescript/ implementation — keeps edge routes faithful
 * to ELK's computed bend points instead of letting smoothstep guess.
 */
function pathFromPoints(points: { x: number; y: number }[], radius = 12): string {
  if (points.length < 2) return '';
  const parts: string[] = [`M ${points[0]!.x} ${points[0]!.y}`];

  const dist = (a: { x: number; y: number }, b: { x: number; y: number }) =>
    Math.hypot(a.x - b.x, a.y - b.y);
  const isPerpendicular = (
    p1: { x: number; y: number }, p2: { x: number; y: number },
    p3: { x: number; y: number }, p4: { x: number; y: number },
  ) => (p1.x === p2.x && p3.y === p4.y) || (p1.y === p2.y && p3.x === p4.x);

  for (let i = 1; i < points.length - 1; i++) {
    const prev = points[i - 1]!;
    const center = points[i]!;
    const next = points[i + 1]!;

    if (!isPerpendicular(prev, center, center, next)) {
      parts.push(`L ${center.x} ${center.y}`);
      continue;
    }
    const r = Math.min(dist(center, prev) / 2, dist(center, next) / 2, radius);
    const isHorizontalIn = prev.y === center.y;
    const xDir = isHorizontalIn ? (prev.x < next.x ? -1 : 1) : (prev.x < next.x ? 1 : -1);
    const yDir = isHorizontalIn ? (prev.y < next.y ? 1 : -1) : (prev.y < next.y ? -1 : 1);

    if (isHorizontalIn) {
      parts.push(`L ${center.x + r * xDir},${center.y} Q ${center.x},${center.y} ${center.x},${center.y + r * yDir}`);
    } else {
      parts.push(`L ${center.x},${center.y + r * yDir} Q ${center.x},${center.y} ${center.x + r * xDir},${center.y}`);
    }
  }
  const last = points[points.length - 1]!;
  parts.push(`L ${last.x} ${last.y}`);
  return parts.join(' ');
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

    let edgePath: string;
    if (data?.points && data.points.length >= 2) {
      // ELK-routed polyline — orthogonal segments with rounded corners.
      edgePath = pathFromPoints(data.points, 12);
    } else {
      [edgePath] = getSmoothStepPath({
        sourceX,
        sourceY,
        targetX,
        targetY,
        sourcePosition,
        targetPosition,
        borderRadius: 14,
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
          opacity: selected ? 1 : 0.7,
          strokeWidth: selected ? 2 : 1.4,
          strokeLinecap: 'round',
          strokeLinejoin: 'round',
          fill: 'none',
          filter: selected ? `drop-shadow(0 0 4px ${edgeColor})` : undefined,
          transition: 'opacity 120ms ease, stroke-width 120ms ease',
          ...style,
        }}
      />
    );
  },
);

BaseEdge.displayName = 'BaseEdge';
