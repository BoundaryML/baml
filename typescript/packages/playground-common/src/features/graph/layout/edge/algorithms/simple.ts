import { uuid } from '@del-wang/utils';

import type { LayoutDirection } from '../../node';
import { type ControlPoint, isInLine, isOnLine } from '../point';

interface GetSimplePathParams {
  isDirectConnect?: boolean;
  source: ControlPoint;
  target: ControlPoint;
  sourceOffset: ControlPoint;
  targetOffset: ControlPoint;
}

const getLineDirection = (
  start: ControlPoint,
  end: ControlPoint,
): LayoutDirection => (start.x === end.x ? 'vertical' : 'horizontal');

/**
 * When two nodes are too close, use the simple path
 *
 * @returns Control points including sourceOffset and targetOffset (not including source and target points).
 */
export const getSimplePath = ({
  isDirectConnect,
  source,
  target,
  sourceOffset,
  targetOffset,
}: GetSimplePathParams): ControlPoint[] => {
  const points: ControlPoint[] = [];
  const sourceDirection = getLineDirection(source, sourceOffset);
  const targetDirection = getLineDirection(target, targetOffset);
  const isHorizontalLayout = sourceDirection === 'horizontal';
  if (isDirectConnect) {
    // Direct connection, return a simple Path
    if (isHorizontalLayout) {
      if (sourceOffset.x <= targetOffset.x) {
        const centerX = (sourceOffset.x + targetOffset.x) / 2;
        return [
          { id: uuid(), x: centerX, y: sourceOffset.y },
          { id: uuid(), x: centerX, y: targetOffset.y },
        ];
      } else {
        const centerY = (sourceOffset.y + targetOffset.y) / 2;
        return [
          sourceOffset,
          { id: uuid(), x: sourceOffset.x, y: centerY },
          { id: uuid(), x: targetOffset.x, y: centerY },
          targetOffset,
        ];
      }
    } else {
      if (sourceOffset.y <= targetOffset.y) {
        const centerY = (sourceOffset.y + targetOffset.y) / 2;
        return [
          { id: uuid(), x: sourceOffset.x, y: centerY },
          { id: uuid(), x: targetOffset.x, y: centerY },
        ];
      } else {
        const centerX = (sourceOffset.x + targetOffset.x) / 2;
        return [
          sourceOffset,
          { id: uuid(), x: centerX, y: sourceOffset.y },
          { id: uuid(), x: centerX, y: targetOffset.y },
          targetOffset,
        ];
      }
    }
  }
  if (sourceDirection === targetDirection) {
    // Same direction, add two points, two endpoints of parallel lines at half the vertical distance
    if (source.y === sourceOffset.y) {
      points.push({
        id: uuid(),
        x: sourceOffset.x,
        y: (sourceOffset.y + targetOffset.y) / 2,
      });
      points.push({
        id: uuid(),
        x: targetOffset.x,
        y: (sourceOffset.y + targetOffset.y) / 2,
      });
    } else {
      points.push({
        id: uuid(),
        x: (sourceOffset.x + targetOffset.x) / 2,
        y: sourceOffset.y,
      });
      points.push({
        id: uuid(),
        x: (sourceOffset.x + targetOffset.x) / 2,
        y: targetOffset.y,
      });
    }
  } else {
    // Different directions, add one point, ensure it's not on the current line segment (to avoid overlap), and there are no turns
    let point = { id: uuid(), x: sourceOffset.x, y: targetOffset.y };
    const inStart = isInLine(point, source, sourceOffset);
    const inEnd = isInLine(point, target, targetOffset);
    if (inStart || inEnd) {
      point = { id: uuid(), x: targetOffset.x, y: sourceOffset.y };
    } else {
      const onStart = isOnLine(point, source, sourceOffset);
      const onEnd = isOnLine(point, target, targetOffset);
      if (onStart && onEnd) {
        point = { id: uuid(), x: targetOffset.x, y: sourceOffset.y };
      }
    }
    points.push(point);
  }
  return [sourceOffset, ...points, targetOffset];
};
