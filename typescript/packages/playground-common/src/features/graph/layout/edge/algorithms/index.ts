import { areLinesSameDirection, isHorizontalFromPosition } from '../edge';
import {
  type ControlPoint,
  getCenterPoints,
  getExpandedRect,
  getOffsetPoint,
  getSidesFromPoints,
  getVerticesFromRectVertex,
  type HandlePosition,
  type NodeRect,
  optimizeInputPoints,
  reducePoints,
} from '../point';
import { getAStarPath } from './a-star';
import { getSimplePath } from './simple';

export interface GetControlPointsParams {
  source: HandlePosition;
  target: HandlePosition;
  sourceRect: NodeRect;
  targetRect: NodeRect;
  /**
   * Minimum spacing between edges and nodes
   */
  offset: number;
}

/**
 * Calculate control points on the optimal path of an edge.
 *
 * Reference article: https://juejin.cn/post/6942727734518874142
 */
export const getControlPoints = ({
  source: oldSource,
  target: oldTarget,
  sourceRect,
  targetRect,
  offset = 20,
}: GetControlPointsParams) => {
  const source: ControlPoint = oldSource;
  const target: ControlPoint = oldTarget;
  let edgePoints: ControlPoint[] = [];
  let optimized: ReturnType<typeof optimizeInputPoints>;

  // 1. Find the starting and ending points after applying the offset
  const sourceOffset = getOffsetPoint(oldSource, offset);
  const targetOffset = getOffsetPoint(oldTarget, offset);
  const expandedSource = getExpandedRect(sourceRect, offset);
  const expandedTarget = getExpandedRect(targetRect, offset);

  // 2. Determine if the two Rects are relatively close or should directly connected
  const minOffset = 2 * offset + 10;
  const isHorizontalLayout = isHorizontalFromPosition(oldSource.position);
  const isSameDirection = areLinesSameDirection(
    source,
    sourceOffset,
    targetOffset,
    target,
  );
  const sides = getSidesFromPoints([
    source,
    target,
    sourceOffset,
    targetOffset,
  ]);
  const isTooClose = isHorizontalLayout
    ? sides.right - sides.left < minOffset
    : sides.bottom - sides.top < minOffset;
  const isDirectConnect = isHorizontalLayout
    ? isSameDirection && source.x < target.x
    : isSameDirection && source.y < target.y;

  if (isTooClose || isDirectConnect) {
    // 3. If the two Rects are relatively close or directly connected, return a simple Path
    edgePoints = getSimplePath({
      source,
      target,
      sourceOffset,
      targetOffset,
      isDirectConnect,
    });
    optimized = optimizeInputPoints({
      source: oldSource,
      target: oldTarget,
      sourceOffset,
      targetOffset,
      edgePoints,
    });
    edgePoints = optimized.edgePoints;
  } else {
    // 3. Find the vertices of the two expanded Rects
    edgePoints = [
      ...getVerticesFromRectVertex(expandedSource, targetOffset),
      ...getVerticesFromRectVertex(expandedTarget, sourceOffset),
    ];
    // 4. Find possible midpoints and intersections
    edgePoints = edgePoints.concat(
      getCenterPoints({
        source: expandedSource,
        target: expandedTarget,
        sourceOffset,
        targetOffset,
      }),
    );
    // 5. Merge nearby coordinate points and remove duplicate coordinate points
    optimized = optimizeInputPoints({
      source: oldSource,
      target: oldTarget,
      sourceOffset,
      targetOffset,
      edgePoints,
    });
    // 6. Find the optimal path
    edgePoints = getAStarPath({
      points: optimized.edgePoints,
      source: optimized.source,
      target: optimized.target,
      sourceRect: getExpandedRect(sourceRect, offset / 2),
      targetRect: getExpandedRect(targetRect, offset / 2),
    });
  }

  return {
    points: reducePoints([optimized.source, ...edgePoints, optimized.target]),
    inputPoints: optimized.edgePoints,
  };
};
