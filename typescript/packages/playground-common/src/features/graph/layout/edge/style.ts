import { deepClone, deepEqual, lastOf } from '@del-wang/utils';
import { getBezierPath, type Position } from '@xyflow/react';

import { flowStore } from '../../../../states/reactflow';

import { getMarkerColors } from '../../../../graph-primitives/edges/Marker';
import type { EdgeLayout, ReactflowEdgeWithData } from '../../../../mock-data/types';
import { getBasePath } from '.';
import { getPathWithRoundCorners } from './edge';

interface EdgeStyle {
  color: string;
  edgeType: 'solid' | 'dashed';
  pathType: 'base' | 'bezier';
}

/**
 * Get the style of the connection line
 *
 * 1. When there are more than 3 edges connecting to both ends of the Node, use multiple colors to distinguish the edges.
 * 2. When the connection line goes backward or connects to a hub Node, use dashed lines to distinguish the edges.
 * 3. When the connection line goes from a hub to a Node, use bezier path.
 */
export const getEdgeStyles = (props: {
  id: string;
  isBackward: boolean;
}): EdgeStyle => {
  const { id, isBackward } = props;
  const idx = parseInt(lastOf(id.split('#')) ?? '0', 10);

  // Get theme-appropriate colors
  const markerColors = getMarkerColors();

  if (isBackward) {
    // Use dashed lines to distinguish the edges when the connection line goes backward or connects to a hub Node
    return { color: markerColors.no, edgeType: 'dashed', pathType: 'base' };
  }
  const edge = flowStore.value.getEdge(id)! as ReactflowEdgeWithData;

  // Safety check: if edge data or port info is missing, use default style
  if (!edge.data || !edge.data.targetPort || !edge.data.sourcePort) {
    return { color: markerColors.base, edgeType: 'solid', pathType: 'base' };
  }

  if (edge.data.targetPort.edges > 2) {
    // Use dashed bezier path when the connection line connects to a hub Node
    return {
      color: markerColors.yes,
      edgeType: 'dashed',
      pathType: 'bezier',
    };
  }
  if (edge.data.sourcePort.edges > 2) {
    // Use multiple colors to distinguish the edges when there are more than 3 edges connecting to both ends of the Node
    return {
      color:
        markerColors.colors[idx % markerColors.colors.length] || markerColors.base,
      edgeType: 'solid',
      pathType: 'base',
    };
  }
  return { color: markerColors.base, edgeType: 'solid', pathType: 'base' };
};

interface ILayoutEdge {
  id: string;
  layout?: EdgeLayout;
  offset: number;
  borderRadius: number;
  pathType: EdgeStyle['pathType'];
  source: string;
  target: string;
  sourceX: number;
  sourceY: number;
  targetX: number;
  targetY: number;
  sourcePosition: Position;
  targetPosition: Position;
}

export function layoutEdge({
  id,
  layout,
  offset,
  borderRadius,
  pathType,
  source,
  target,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
}: ILayoutEdge): EdgeLayout {
  // CRITICAL: If we have ELK-calculated points (inputPoints), use them and don't recalculate
  const hasElkRouting = layout?.inputPoints && layout.inputPoints.length > 1;

  if (hasElkRouting) {

    // Use ELK routing - only rebuild path if points changed
    const reBuildPathDeps = layout?.points;
    const needReBuildPath = !deepEqual(
      reBuildPathDeps,
      layout?.deps?.reBuildPathDeps,
    );

    if (needReBuildPath || !layout?.path) {
      // Generate path from ELK points
      layout!.path = getPathWithRoundCorners(layout!.points, borderRadius);

    }

    const relayoutDeps = [sourceX, sourceY, targetX, targetY];
    layout!.deps = deepClone({ relayoutDeps, reBuildPathDeps });
    return layout!;
  }

  // No ELK routing - use default behavior
  const relayoutDeps = [sourceX, sourceY, targetX, targetY];
  const needRelayout = !deepEqual(relayoutDeps, layout?.deps?.relayoutDeps);
  const reBuildPathDeps = layout?.points;
  const needReBuildPath = !deepEqual(
    reBuildPathDeps,
    layout?.deps?.reBuildPathDeps,
  );
  let newLayout = layout;
  if (needRelayout) {
    newLayout = _layoutEdge({
      id,
      offset,
      borderRadius,
      pathType,
      source,
      target,
      sourceX,
      sourceY,
      targetX,
      targetY,
      sourcePosition,
      targetPosition,
    });
  } else if (needReBuildPath) {
    newLayout = _layoutEdge({
      layout,
      id,
      offset,
      borderRadius,
      pathType,
      source,
      target,
      sourceX,
      sourceY,
      targetX,
      targetY,
      sourcePosition,
      targetPosition,
    });
  }
  newLayout!.deps = deepClone({ relayoutDeps, reBuildPathDeps });
  return newLayout!;
}

function _layoutEdge({
  id,
  layout,
  offset,
  borderRadius,
  pathType,
  source,
  target,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
}: ILayoutEdge): EdgeLayout {
  const _pathType: EdgeStyle['pathType'] = pathType;
  if (_pathType === 'bezier') {
    const [path, labelX, labelY] = getBezierPath({
      sourceX,
      sourceY,
      targetX,
      targetY,
      sourcePosition,
      targetPosition,
    });
    const points = [
      {
        id: 'source-' + id,
        x: sourceX,
        y: sourceY,
      },
      {
        id: 'target-' + id,
        x: targetX,
        y: targetY,
      },
    ];
    return {
      path,
      points,
      inputPoints: points,
      labelPosition: {
        x: labelX,
        y: labelY,
      },
    };
  }

  if ((layout?.points?.length ?? 0) > 1) {
    layout!.path = getPathWithRoundCorners(layout!.points, borderRadius);
    return layout!;
  }

  return getBasePath({
    id,
    offset,
    borderRadius,
    source,
    target,
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  });
}
