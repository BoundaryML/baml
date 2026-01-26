import { uuid } from '@del-wang/utils';
import { Position, type XYPosition } from '@xyflow/react';

import type { ControlPoint, HandlePosition } from './point';

export interface ILine {
  start: ControlPoint;
  end: ControlPoint;
}

export const isHorizontalFromPosition = (position: Position) => {
  return [Position.Left, Position.Right].includes(position);
};

export const isConnectionBackward = (props: {
  source: HandlePosition;
  target: HandlePosition;
}) => {
  const { source, target } = props;
  const isHorizontal = isHorizontalFromPosition(source.position);
  let isBackward = false;
  if (isHorizontal) {
    if (source.x > target.x) {
      isBackward = true;
    }
  } else {
    if (source.y > target.y) {
      isBackward = true;
    }
  }
  return isBackward;
};

/**
 * Get the distance between two points
 */
export const distance = (p1: ControlPoint, p2: ControlPoint) => {
  return Math.hypot(p2.x - p1.x, p2.y - p1.y);
};

/**
 * Get the midpoint of the line segment
 */
export const getLineCenter = (
  p1: ControlPoint,
  p2: ControlPoint,
): ControlPoint => {
  return {
    id: uuid(),
    x: (p1.x + p2.x) / 2,
    y: (p1.y + p2.y) / 2,
  };
};

/**
 * Whether the line segment contains point
 */
export const isLineContainsPoint = (
  start: ControlPoint,
  end: ControlPoint,
  p: ControlPoint,
) => {
  return (
    (start.x === end.x &&
      p.x === start.x &&
      p.y <= Math.max(start.y, end.y) &&
      p.y >= Math.min(start.y, end.y)) ||
    (start.y === end.y &&
      p.y === start.y &&
      p.x <= Math.max(start.x, end.x) &&
      p.x >= Math.min(start.x, end.x))
  );
};

/**
 * Generates an SVG path for an edge based on the control points.
 *
 * The line between two control points is straight, and each control point represents a turning point with rounded corners.
 *
 * @param points An array of points representing the endpoints and control points of the edge.
 *
 * - At least 2 points are required.
 * - The points should be ordered starting from the input endpoint and ending at the output endpoint.
 *
 * @param radius The radius of the rounded corners at each turning point.
 *
 */
export function getPathWithRoundCorners(
  points: ControlPoint[],
  radius: number,
): string {
  if (points.length < 2) {
    throw new Error('At least 2 points are required.');
  }

  function getRoundCorner(
    center: ControlPoint,
    p1: ControlPoint,
    p2: ControlPoint,
    radius: number,
  ) {
    const { x, y } = center;

    if (!areLinesPerpendicular(p1, center, center, p2)) {
      // The two line segments are not vertical, and return directly to the straight line
      return `L ${x} ${y}`;
    }

    const d1 = distance(center, p1);
    const d2 = distance(center, p2);
    // eslint-disable-next-line no-param-reassign
    radius = Math.min(d1 / 2, d2 / 2, radius);

    const isHorizontal = p1.y === y;

    const xDir = isHorizontal ? (p1.x < p2.x ? -1 : 1) : p1.x < p2.x ? 1 : -1;
    const yDir = isHorizontal ? (p1.y < p2.y ? 1 : -1) : p1.y < p2.y ? -1 : 1;

    if (isHorizontal) {
      return `L ${x + radius * xDir},${y}Q ${x},${y} ${x},${y + radius * yDir}`;
    }

    return `L ${x},${y + radius * yDir}Q ${x},${y} ${x + radius * xDir},${y}`;
  }

  const path: string[] = [];
  for (let i = 0; i < points.length; i++) {
    if (i === 0) {
      // Starting
      const p = points[i]!;
      path.push(`M ${p.x} ${p.y}`);
    } else if (i === points.length - 1) {
      // Ending
      const p = points[i]!;
      path.push(`L ${p.x} ${p.y}`);
    } else {
      const center = points[i]!;
      const prev = points[i - 1]!;
      const next = points[i + 1]!;
      path.push(getRoundCorner(center, prev, next, radius));
    }
  }

  return path.join(' ');
}

/**
 * Get the longest line segment on the folding line
 */
export function getLongestLine(
  points: ControlPoint[],
): [ControlPoint, ControlPoint] {
  if (points.length < 2) {
    throw new Error('At least 2 points are required.');
  }
  let longestLine: [ControlPoint, ControlPoint] = [points[0]!, points[1]!];
  let longestDistance = distance(...longestLine);
  for (let i = 1; i < points.length - 1; i++) {
    const _distance = distance(points[i]!, points[i + 1]!);
    if (_distance > longestDistance) {
      longestDistance = _distance;
      longestLine = [points[i]!, points[i + 1]!];
    }
  }
  return longestLine;
}

/**
 * Calculate the position of a label on the polyline.
 *
 * It first finds the midpoint, and if the number of points is odd, it then finds the longest path.
 */
export function getLabelPosition(
  points: ControlPoint[],
  minGap = 20,
): XYPosition {
  if (points.length % 2 === 0) {
    // Find the midpoint of the polyline
    const middleP1 = points[points.length / 2 - 1]!;
    const middleP2 = points[points.length / 2]!;
    if (distance(middleP1, middleP2) > minGap) {
      return getLineCenter(middleP1, middleP2);
    }
  }
  const [start, end] = getLongestLine(points);
  return {
    x: (start.x + end.x) / 2,
    y: (start.y + end.y) / 2,
  };
}

/**
 * Determines whether two line segments are perpendicular (assuming line segments are either horizontal or vertical).
 */
export function areLinesPerpendicular(
  p1: ControlPoint,
  p2: ControlPoint,
  p3: ControlPoint,
  p4: ControlPoint,
): boolean {
  return (p1.x === p2.x && p3.y === p4.y) || (p1.y === p2.y && p3.x === p4.x);
}

/**
 * Determines whether two line segments are parallel (assuming line segments are either horizontal or vertical).
 */
export function areLinesParallel(
  p1: ControlPoint,
  p2: ControlPoint,
  p3: ControlPoint,
  p4: ControlPoint,
) {
  return (p1.x === p2.x && p3.x === p4.x) || (p1.y === p2.y && p3.y === p4.y);
}

/**
 * Determines whether two lines are in the same direction (assuming line segments are either horizontal or vertical).
 */
export function areLinesSameDirection(
  p1: ControlPoint,
  p2: ControlPoint,
  p3: ControlPoint,
  p4: ControlPoint,
) {
  return (
    (p1.x === p2.x && p3.x === p4.x && (p1.y - p2.y) * (p3.y - p4.y) > 0) ||
    (p1.y === p2.y && p3.y === p4.y && (p1.x - p2.x) * (p3.x - p4.x) > 0)
  );
}

/**
 * Determines whether two lines are in reverse direction (assuming line segments are either horizontal or vertical).
 */
export function areLinesReverseDirection(
  p1: ControlPoint,
  p2: ControlPoint,
  p3: ControlPoint,
  p4: ControlPoint,
) {
  return (
    (p1.x === p2.x && p3.x === p4.x && (p1.y - p2.y) * (p3.y - p4.y) < 0) ||
    (p1.y === p2.y && p3.y === p4.y && (p1.x - p2.x) * (p3.x - p4.x) < 0)
  );
}

export function getAngleBetweenLines(
  p1: ControlPoint,
  p2: ControlPoint,
  p3: ControlPoint,
  p4: ControlPoint,
) {
  // Calculate the vectors of the two line segments
  const v1 = { x: p2.x - p1.x, y: p2.y - p1.y };
  const v2 = { x: p4.x - p3.x, y: p4.y - p3.y };

  // Calculate the dot product of the two vectors
  const dotProduct = v1.x * v2.x + v1.y * v2.y;

  // Calculate the magnitudes of the two vectors
  const magnitude1 = Math.sqrt(v1.x ** 2 + v1.y ** 2);
  const magnitude2 = Math.sqrt(v2.x ** 2 + v2.y ** 2);

  // Calculate the cosine of the angle
  const cosine = dotProduct / (magnitude1 * magnitude2);

  // Calculate the angle in radians
  const angleInRadians = Math.acos(cosine);

  // Convert the angle to degrees
  const angleInDegrees = (angleInRadians * 180) / Math.PI;

  return angleInDegrees;
}
