import { BaseEdge as RFBaseEdge, type EdgeProps, type Edge, getSmoothStepPath } from '@xyflow/react';
import { memo } from 'react';
import { getMarkerColors } from './Marker';
import type { WorkflowEdgeData } from '../types';

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

    const [edgePath] = getSmoothStepPath({
      sourceX,
      sourceY,
      targetX,
      targetY,
      sourcePosition,
      targetPosition,
      borderRadius: 12,
    });

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
