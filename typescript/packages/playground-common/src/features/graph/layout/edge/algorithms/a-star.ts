import { areLinesReverseDirection, areLinesSameDirection } from '../edge';
import {
  type ControlPoint,
  isEqualPoint,
  isSegmentCrossingRect,
  type NodeRect,
} from '../point';

interface GetAStarPathParams {
  /**
   * Collection of potential control points between `sourceOffset` and `targetOffset`, excluding the `source` and `target` points.
   */
  points: ControlPoint[];
  source: ControlPoint;
  target: ControlPoint;
  /**
   * Node size information for the `source` and `target`, used to optimize edge routing without intersecting nodes.
   */
  sourceRect: NodeRect;
  targetRect: NodeRect;
}

/**
 * Utilizes the [A\* search algorithm](https://en.wikipedia.org/wiki/A*_search_algorithm) combined with
 * [Manhattan Distance](https://simple.wikipedia.org/wiki/Manhattan_distance) to find the optimal path for edges.
 *
 * @returns Control points including sourceOffset and targetOffset (not including source and target points).
 */
export const getAStarPath = ({
  points,
  source,
  target,
  sourceRect,
  targetRect,
}: GetAStarPathParams): ControlPoint[] => {
  if (points.length < 3) {
    return points;
  }
  const start = points[0]!;
  const end = points[points.length - 1]!;
  const openSet: ControlPoint[] = [start];
  const closedSet: Set<ControlPoint> = new Set();
  const cameFrom: Map<ControlPoint, ControlPoint> = new Map();
  const gScore: Map<ControlPoint, number> = new Map().set(start, 0);
  const fScore: Map<ControlPoint, number> = new Map().set(
    start,
    heuristicCostEstimate({
      from: start,
      to: start,
      start,
      end,
      source,
      target,
    }),
  );

  while (openSet.length) {
    let current: ControlPoint = openSet[0]!;
    let currentIdx = 0;
    let lowestFScore = fScore.get(current) ?? 0;
    for (let idx = 1; idx < openSet.length; idx++) {
      const p = openSet[idx]!;
      const score = fScore.get(p) ?? 0;
      if (score < lowestFScore) {
        lowestFScore = score;
        current = p;
        currentIdx = idx;
      }
    }

    if (current === end) {
      return buildPath(cameFrom, current);
    }

    openSet.splice(currentIdx, 1);
    closedSet.add(current);

    const curFScore = fScore.get(current) ?? 0;
    const previous = cameFrom.get(current);
    const neighbors = getNextNeighborPoints({
      points,
      previous,
      current,
      sourceRect,
      targetRect,
    });
    for (const neighbor of neighbors) {
      if (closedSet.has(neighbor)) {
        continue;
      }
      const neighborGScore = gScore.get(neighbor) ?? 0;
      const tentativeGScore = curFScore + estimateDistance(current, neighbor);
      if (openSet.includes(neighbor) && tentativeGScore >= neighborGScore) {
        continue;
      }
      openSet.push(neighbor);
      cameFrom.set(neighbor, current);
      gScore.set(neighbor, tentativeGScore);
      fScore.set(
        neighbor,
        neighborGScore +
        heuristicCostEstimate({
          from: current,
          to: neighbor,
          start,
          end,
          source,
          target,
        }),
      );
    }
  }
  return [start, end];
};

const buildPath = (
  cameFrom: Map<ControlPoint, ControlPoint>,
  current: ControlPoint,
): ControlPoint[] => {
  const path = [current];

  let previous = cameFrom.get(current);
  while (previous) {
    path.push(previous);
    previous = cameFrom.get(previous);
  }

  return path.reverse();
};

interface GetNextNeighborPointsParams {
  points: ControlPoint[];
  previous?: ControlPoint;
  current: ControlPoint;
  sourceRect: NodeRect;
  targetRect: NodeRect;
}

/**
 * Get the set of possible neighboring points for the current control point
 *
 * - The line is in a horizontal or vertical direction
 * - The line does not intersect with the two end nodes
 * - The line does not overlap with the previous line segment in reverse direction
 */
export const getNextNeighborPoints = ({
  points,
  previous,
  current,
  sourceRect,
  targetRect,
}: GetNextNeighborPointsParams): ControlPoint[] => {
  return points.filter((p) => {
    if (p === current) {
      return false;
    }
    // The connection is in the horizontal or vertical direction
    const rightDirection = p.x === current.x || p.y === current.y;
    // Reverse direction with the previous line segment (overlap)
    const reverseDirection = previous
      ? areLinesReverseDirection(previous, current, current, p)
      : false;
    return (
      rightDirection && // The line is in a horizontal or vertical direction
      !reverseDirection && // The line does not overlap with the previous line segment in reverse direction
      !isSegmentCrossingRect(p, current, sourceRect) && // Does not intersect with sourceNode
      !isSegmentCrossingRect(p, current, targetRect) // Does not intersect with targetNode
    );
  });
};

interface HeuristicCostParams {
  from: ControlPoint;
  to: ControlPoint;
  start: ControlPoint;
  end: ControlPoint;
  source: ControlPoint;
  target: ControlPoint;
}

/**
 * Connection point distance loss function
 *
 * - The smaller the sum of distances, the better
 * - The closer the start and end line segments are in direction, the better
 * - The better the inflection point is symmetric or centered in the line segment
 */
const heuristicCostEstimate = ({
  from,
  to,
  start,
  end,
  source,
  target,
}: HeuristicCostParams): number => {
  const base = estimateDistance(to, start) + estimateDistance(to, end);
  const startCost = isEqualPoint(from, start)
    ? areLinesSameDirection(from, to, source, start)
      ? -base / 2
      : 0
    : 0;
  const endCost = isEqualPoint(to, end)
    ? areLinesSameDirection(from, to, end, target)
      ? -base / 2
      : 0
    : 0;
  return base + startCost + endCost;
};

/**
 * Calculate the estimated distance between two points
 *
 * Manhattan distance: the sum of horizontal and vertical distances, faster calculation speed
 */
const estimateDistance = (p1: ControlPoint, p2: ControlPoint): number =>
  Math.abs(p1.x - p2.x) + Math.abs(p1.y - p2.y);
