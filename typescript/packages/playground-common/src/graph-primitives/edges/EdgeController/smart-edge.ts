import { uuid } from '@del-wang/utils';

import type { ReactflowEdgeWithData } from '../../../mock-data/types';
import {
  areLinesReverseDirection,
  distance,
  type ILine,
  isHorizontalFromPosition,
  isLineContainsPoint,
} from '../../../features/graph/layout/edge/edge';
import {
  type ControlPoint,
  getOffsetPoint,
  reducePoints,
} from '../../../features/graph/layout/edge/point';
import { flowStore } from '../../../states/reactflow';

import { rebuildEdge } from '../BaseEdge/useRebuildEdge';
import type { EdgeControllersParams } from '.';

interface EdgeContext extends EdgeControllersParams {
  source: ControlPoint;
  target: ControlPoint;
  sourceOffset: ControlPoint;
  targetOffset: ControlPoint;
}

export const getEdgeContext = (props: EdgeControllersParams): EdgeContext => {
  const { points, offset, sourcePosition, targetPosition } = props;
  const source = points[0]!;
  const target = points[points.length - 1]!;
  const sourceOffset = getOffsetPoint(
    { ...source, position: sourcePosition },
    offset,
  );
  const targetOffset = getOffsetPoint(
    { ...target, position: targetPosition },
    offset,
  );
  return { ...props, source, target, sourceOffset, targetOffset };
};

export class SmartEdge {
  static draggingEdge?: {
    dragId: string;
    dragFrom?: string;
    target?: {
      start: ControlPoint;
      end: ControlPoint;
    };
    start: ControlPoint;
    end: ControlPoint;
  };

  public idx: number;
  public start: ControlPoint;
  public end: ControlPoint;
  public distance: number;
  public ctx: EdgeContext;

  public previous?: SmartEdge;
  public next?: SmartEdge;

  constructor(options: {
    idx: number;
    start: ControlPoint;
    end: ControlPoint;
    ctx: EdgeContext;
    previous?: SmartEdge;
    next?: SmartEdge;
  }) {
    this.idx = options.idx;
    this.start = options.start;
    this.end = options.end;
    this.ctx = options.ctx;
    this.previous = options.previous;
    this.next = options.next;
    this.distance = distance(this.start, this.end);
  }

  get isHorizontalLayout() {
    return isHorizontalFromPosition(this.ctx.sourcePosition);
  }

  get isHorizontalLine() {
    return this.start.y === this.end.y;
  }

  get isSource() {
    return this.idx === 0;
  }

  get isSourceOffset() {
    return this.idx === 1;
  }

  get isTarget() {
    return this.idx === this.ctx.points.length - 2;
  }

  get isTargetOffset() {
    return this.idx === this.ctx.points.length - 3;
  }

  get isStartFixed() {
    return this.isSource || this.isSourceOffset;
  }

  get isEndFixed() {
    return this.isTarget || this.isTargetOffset;
  }

  get minHandlerWidth() {
    return this.ctx.handlerWidth + 2 * this.ctx.offset;
  }

  /**
   * Whether the edge can be dragged.
   */
  get canDrag() {
    if (this.isStartFixed || this.isEndFixed) {
      // The connection lines on both ends of the node should not be dragged unless the edge can be split.
      return this.canSplit;
    }
    return this.distance >= this.minHandlerWidth;
  }

  /**
   * Whether the edge can be split.
   */
  get canSplit() {
    return this.distance >= this.minHandlerWidth + 2 * this.ctx.offset;
  }

  /**
   * Splits the edge automatically when fixed endpoints exist at both ends.
   */
  splitPoints(
    dragId: string,
    from: ILine,
    _to: ILine,
    minGap = 10,
  ): ControlPoint[] | undefined {
    const startPoints = this.ctx.points.slice(0, this.idx + 1);
    const endPoints = this.ctx.points.slice(this.idx + 1);

    let to = { ..._to };
    const isTempDraggingEdge =
      SmartEdge.draggingEdge?.dragId === dragId &&
      SmartEdge.draggingEdge?.target;
    if (isTempDraggingEdge) {
      // Adjust the offset based on the previous dragging edge position.
      to = {
        start: {
          id: uuid(),
          x:
            SmartEdge.draggingEdge!.target!.start.x +
            (to.start.x - from.start.x),
          y:
            SmartEdge.draggingEdge!.target!.start.y +
            (to.start.y - from.start.y),
        },
        end: {
          id: uuid(),
          x: SmartEdge.draggingEdge!.target!.end.x + (to.end.x - from.end.x),
          y: SmartEdge.draggingEdge!.target!.end.y + (to.end.y - from.end.y),
        },
      };
    }

    // Whether the edge needs to be split.
    let needSplit = false;
    // Whether to initiate the splitting of this edge.
    let startSplit = false;

    const sourceDelta = this.isHorizontalLine
      ? Math.abs(this.ctx.source.y - to.start.y)
      : Math.abs(this.ctx.source.x - to.start.x);
    const targetDelta = this.isHorizontalLine
      ? Math.abs(this.ctx.target.y - to.end.y)
      : Math.abs(this.ctx.target.x - to.end.x);
    const moveDelta = this.isHorizontalLine
      ? Math.abs(from.start.y - to.start.y)
      : Math.abs(from.start.x - to.start.x);

    if (this.isSource) {
      needSplit = true;
      if (sourceDelta > minGap) {
        startSplit = true;
      }
    } else if (this.isTarget) {
      needSplit = true;
      if (targetDelta > minGap) {
        startSplit = true;
      }
    } else {
      if (this.isSourceOffset && sourceDelta < this.ctx.offset) {
        needSplit = true;
        if (moveDelta > minGap) {
          startSplit = true;
        }
      } else if (this.isTargetOffset && targetDelta < this.ctx.offset) {
        needSplit = true;
        if (moveDelta > minGap) {
          startSplit = true;
        }
      }
    }

    if (!needSplit) {
      return;
    }

    const _offset = (distance(from.start, from.end) - this.minHandlerWidth) / 2;
    if (this.isHorizontalLine) {
      const direction = from.start.x < from.end.x ? 1 : -1;
      const offset = _offset * direction;
      if (!startSplit) {
        SmartEdge.draggingEdge = {
          dragId,
          start: from.start,
          end: from.end,
          target: { start: to.start, end: to.end },
        };
        return this.ctx.points;
      }
      SmartEdge.draggingEdge = {
        dragId,
        start: { id: uuid(), x: from.start.x + offset, y: to.start.y },
        end: { id: uuid(), x: from.end.x - offset, y: to.start.y },
      };
      return [
        ...startPoints,
        { id: uuid(), x: from.start.x + offset, y: from.start.y },
        SmartEdge.draggingEdge.start,
        SmartEdge.draggingEdge.end,
        { id: uuid(), x: from.end.x - offset, y: from.start.y },
        ...endPoints,
      ];
    } else {
      const direction = from.start.y < from.end.y ? 1 : -1;
      const offset = _offset * direction;
      if (!startSplit) {
        SmartEdge.draggingEdge = {
          dragId,
          start: from.start,
          end: from.end,
          target: { start: to.start, end: to.end },
        };
        return this.ctx.points;
      }
      SmartEdge.draggingEdge = {
        dragId,
        start: { id: uuid(), x: to.start.x, y: from.start.y + offset },
        end: { id: uuid(), x: to.start.x, y: from.end.y - offset },
      };
      return [
        ...startPoints,
        { id: uuid(), x: from.start.x, y: from.start.y + offset },
        SmartEdge.draggingEdge.start,
        SmartEdge.draggingEdge.end,
        { id: uuid(), x: from.start.x, y: from.end.y - offset },
        ...endPoints,
      ];
    }
  }

  /**
   * Merges endpoints of edges with close distances.
   */
  mergePoints(
    dragId: string,
    from: ILine,
    to: ILine,
    minGap = 10,
  ): ControlPoint[] | undefined {
    const startPoints = this.previous
      ? this.ctx.points.slice(0, this.previous.idx)
      : [];
    const endPoints = this.next
      ? this.ctx.points.slice(this.next.idx + 1 + 1)
      : [];
    if (this.isHorizontalLine) {
      const fromY = from.start.y;
      const toY = to.start.y;
      const preY = this.previous?.start.y;
      const nextY = this.next?.start.y;
      // Find the edge closest to the target coordinates
      const targetY = preY
        ? nextY
          ? Math.abs(toY - preY) < Math.abs(toY - nextY)
            ? preY
            : nextY
          : preY
        : nextY!;
      // Determine whether merging is necessary (1. Near adsorption objects 2. Close enough)
      const currentDistance = Math.abs(toY - targetY);
      const needMerge =
        Math.abs(fromY - targetY) > currentDistance && currentDistance < minGap;
      if (needMerge) {
        // Merge to new endpoint
        if (preY === nextY && preY === targetY) {
          // Previous, Current, Next merged into a straight line
          SmartEdge.draggingEdge = {
            dragId,
            start: this.previous!.start,
            end: this.next!.end,
          };
          return [
            ...startPoints,
            SmartEdge.draggingEdge.start,
            SmartEdge.draggingEdge.end,
            ...endPoints,
          ];
        } else if (preY === targetY) {
          if (this.next) {
            // Previous, Current merged into a straight line
            SmartEdge.draggingEdge = {
              dragId,
              start: this.previous!.start,
              end: { id: uuid(), x: from.end.x, y: preY }, // New endpoint (projection)
            };
            return [
              ...startPoints,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              this.next!.start,
              this.next!.end,
              ...endPoints,
            ];
          } else {
            // The next edge is empty
            SmartEdge.draggingEdge = {
              dragId,
              start: this.previous!.start,
              end: { id: uuid(), x: from.end.x, y: preY }, // New endpoint (projection)
            };
            return [
              ...startPoints,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              this.ctx.target,
              ...endPoints,
            ];
          }
        } else {
          if (this.previous) {
            // Current, Next merged into a straight line
            SmartEdge.draggingEdge = {
              dragId,
              start: { id: uuid(), x: from.start.x, y: nextY! }, // New endpoint (projection)
              end: this.next!.end,
            };
            return [
              ...startPoints,
              this.previous!.start,
              this.previous!.end,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              ...endPoints,
            ];
          } else {
            // The previous edge is empty
            SmartEdge.draggingEdge = {
              dragId,
              start: { id: uuid(), x: from.start.x, y: nextY! }, // New endpoint (projection)
              end: this.next!.end,
            };
            return [
              ...startPoints,
              this.ctx.source,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              ...endPoints,
            ];
          }
        }
      }
    } else {
      const fromX = from.start.x;
      const toX = to.start.x;
      const preX = this.previous?.start.x;
      const nextX = this.next?.start.x;
      // Find the edge closest to the target coordinates
      const targetX = preX
        ? nextX
          ? Math.abs(toX - preX) < Math.abs(toX - nextX)
            ? preX
            : nextX
          : preX
        : nextX!;
      // Determine whether merging is necessary (1. Near adsorption objects 2. Close enough)
      const currentDistance = Math.abs(toX - targetX);
      const needMerge =
        Math.abs(fromX - targetX) > currentDistance && currentDistance < minGap;
      if (needMerge) {
        // Merge to new endpoint
        if (preX === nextX && preX === targetX) {
          // Previous, Current, Next merged into a straight line
          SmartEdge.draggingEdge = {
            dragId,
            start: this.previous!.start,
            end: this.next!.end,
          };
          return [
            ...startPoints,
            SmartEdge.draggingEdge.start,
            SmartEdge.draggingEdge.end,
            ...endPoints,
          ];
        } else if (preX === targetX) {
          if (this.next) {
            // Previous, Current merged into a straight line
            SmartEdge.draggingEdge = {
              dragId,
              start: this.previous!.start,
              end: { id: uuid(), x: preX, y: from.end.y }, // New endpoint (projection)
            };
            return [
              ...startPoints,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              this.next!.start,
              this.next!.end,
              ...endPoints,
            ];
          } else {
            // The next edge is empty
            SmartEdge.draggingEdge = {
              dragId,
              start: this.previous!.start,
              end: { id: uuid(), x: preX, y: from.end.y }, // New endpoint (projection)
            };
            return [
              ...startPoints,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              this.ctx.target,
              ...endPoints,
            ];
          }
        } else {
          if (this.previous) {
            // Current, Next merged into a straight line
            SmartEdge.draggingEdge = {
              dragId,
              start: { id: uuid(), x: nextX!, y: from.start.y }, // New endpoint (projection)
              end: this.next!.end,
            };
            return [
              ...startPoints,
              this.previous!.start,
              this.previous!.end,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              ...endPoints,
            ];
          } else {
            // The previous edge is empty
            SmartEdge.draggingEdge = {
              dragId,
              start: { id: uuid(), x: nextX!, y: from.start.y }, // New endpoint (projection)
              end: this.next!.end,
            };
            return [
              ...startPoints,
              this.ctx.source,
              SmartEdge.draggingEdge.start,
              SmartEdge.draggingEdge.end,
              ...endPoints,
            ];
          }
        }
      }
    }
  }

  /**
   * Check whether the path is valid.
   *
   * 1. Invalid if there is an overlapping path.
   * 2. Invalid if it does not contain node offset endpoints.
   */
  isValidPoints = (points: ControlPoint[]): boolean => {
    if (points.length < 4) {
      // Paths with less than 3 edges are always valid.
      return true;
    }
    const edges: ILine[] = [
      { start: points[0]!, end: points[1]! },
      { start: points[1]!, end: points[2]! },
      { start: points[points.length - 3]!, end: points[points.length - 2]! },
      { start: points[points.length - 2]!, end: points[points.length - 1]! },
    ];
    // Invalid if it does not contain node offset endpoints.
    const e0 = edges[0]!;
    const e1 = edges[1]!;
    const e2 = edges[2]!;
    const e3 = edges[3]!;
    if (
      !isLineContainsPoint(e0.start, e0.end, this.ctx.sourceOffset) ||
      !isLineContainsPoint(e3.start, e3.end, this.ctx.targetOffset)
    ) {
      return false;
    }
    // Invalid if there is an overlapping path.
    if (
      areLinesReverseDirection(e0.start, e0.end, e1.start, e1.end) ||
      areLinesReverseDirection(e2.start, e2.end, e3.start, e3.end)
    ) {
      return false;
    }
    return true;
  };

  rebuildEdge = (points: ControlPoint[]) => {
    const edge = flowStore.value.getEdge(this.ctx.id)! as ReactflowEdgeWithData;
    edge.data!.layout!.points = reducePoints(points);
    rebuildEdge(this.ctx.id);
  };

  onDragging = ({
    dragId,
    from,
    to,
  }: {
    dragId: string;
    dragFrom: string;
    from: ILine;
    to: ILine;
  }) => {
    if (distance(from.start, to.start) < 0.00001) {
      // The drag distance is very small, only refresh
      return this.rebuildEdge(this.ctx.points);
    }
    // Automatically split edges if there are fixed endpoints at both ends
    if (this.isStartFixed || this.isEndFixed) {
      const splittedPoints = this.splitPoints(dragId, from, to);
      if (splittedPoints) {
        return this.rebuildEdge(splittedPoints);
      }
    }
    // Merge nearby edges
    const mergedPoints = this.mergePoints(dragId, from, to);
    if (mergedPoints && this.isValidPoints(mergedPoints)) {
      return this.rebuildEdge(mergedPoints);
    }
    // Update current edge coordinates
    const { x: targetX, y: targetY } = to.start;
    if (this.isHorizontalLine) {
      this.start.y = targetY;
      this.end.y = targetY;
    } else {
      this.start.x = targetX;
      this.end.x = targetX;
    }
    SmartEdge.draggingEdge = { dragId, start: this.start, end: this.end };
    this.rebuildEdge(this.ctx.points);
  };
}
