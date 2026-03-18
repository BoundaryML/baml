import { BaseEdge as RFBaseEdge, type EdgeProps, getSmoothStepPath } from '@xyflow/react';
import { memo } from 'react';
import { getMarkerColors } from './Marker';

export const BaseEdge = memo<EdgeProps>(
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
    label,
    labelStyle,
    labelShowBg,
    labelBgStyle,
    labelBgPadding,
    labelBgBorderRadius,
    interactionWidth,
  }) => {
    const colors = getMarkerColors();

    const [edgePath, labelX, labelY] = getSmoothStepPath({
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
        label={label}
        labelBgBorderRadius={labelBgBorderRadius}
        labelBgPadding={labelBgPadding}
        labelBgStyle={labelBgStyle}
        labelShowBg={labelShowBg}
        labelStyle={labelStyle}
        labelX={labelX}
        labelY={labelY}
        markerEnd={`url('#${colors.base.replace('#', '')}')`}
        markerStart={markerStart}
        path={edgePath}
        style={{
          ...style,
          stroke: colors.base,
          opacity: selected ? 1 : 0.6,
          strokeWidth: selected ? 2 : 1.5,
        }}
      />
    );
  },
);

BaseEdge.displayName = 'BaseEdge';
