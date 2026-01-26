/* eslint-disable react-hooks/exhaustive-deps */

import type { XYPosition } from '@xyflow/react';
import { useEffect, useRef } from 'react';
import { useXState, XSta } from 'xsta';

import { isEqualPoint } from '../../../features/graph/layout/edge/point';
import { SmartEdge } from './smart-edge';

interface UseDraggableParams {
  edge: SmartEdge;
  dragId?: string;
  onDragStart?: VoidFunction;
  onDragging?: (
    dragId: string,
    dragFrom: string,
    position: XYPosition,
    delta: XYPosition,
  ) => void;
  onDragEnd?: VoidFunction;
}

let _id = 0;
export const useEdgeDraggable = (props: UseDraggableParams) => {
  const dragRef = useRef(null);
  const propsRef = useRef(props);
  propsRef.current = props;

  const isDraggingEdge = () =>
    SmartEdge.draggingEdge &&
    isEqualPoint(SmartEdge.draggingEdge?.start, props.edge.start) &&
    isEqualPoint(SmartEdge.draggingEdge?.end, props.edge.end);

  const dragFrom = useRef((_id++).toString()).current;
  const dragId = isDraggingEdge() ? SmartEdge.draggingEdge!.dragId : dragFrom;
  const isDraggingKey = 'useEdgeDraggable-isDragging-' + dragId;
  const startPositionKey = 'useEdgeDraggable-startPosition-' + dragId;
  const [_, setIsDragging] = useXState(isDraggingKey, false);
  const [__, setStartPosition] = useXState(startPositionKey, [0, 0]);
  const getIsDragging = () => {
    if (XSta.has(isDraggingKey)) {
      return XSta.get(isDraggingKey);
    }
    return false;
  };
  const getStartPosition = () => {
    if (XSta.has(startPositionKey)) {
      return XSta.get(startPositionKey);
    }
    return [0, 0];
  };

  const onDragStart = (event: MouseEvent) => {
    if (getIsDragging()) {
      return;
    }
    if (event.button !== 0) {
      // Not a left mouse button click event
      return;
    }
    if (event.target !== dragRef.current) {
      // Not clicked on the current element
      return;
    }
    SmartEdge.draggingEdge = {
      dragId,
      dragFrom,
      start: propsRef.current.edge.start,
      end: propsRef.current.edge.end,
    };
    setIsDragging(true);
    setStartPosition([event.clientX, event.clientY]);
    propsRef.current.onDragStart?.();
  };

  const onDragEnd = () => {
    setGlobalCursorStyle('auto');
    SmartEdge.draggingEdge = undefined;
    propsRef.current.onDragEnd?.();
    // dispose states
    for (const key of XSta.keys()) {
      if (key.includes('useEdgeDraggable-')) {
        XSta.delete(key);
      }
    }
  };

  const onDragging = (event: MouseEvent) => {
    if (!getIsDragging()) {
      return;
    }
    const { start, end } = propsRef.current.edge;
    const isHorizontal = start.y === end.y;
    setGlobalCursorStyle(isHorizontal ? 'row-resize' : 'col-resize');
    if (event.buttons !== 1) {
      // Ending drag because it's not a left mouse button drag event
      return onDragEnd();
    }
    propsRef.current.onDragging?.(
      dragId,
      dragFrom,
      {
        x: event.clientX,
        y: event.clientY,
      },
      {
        x: event.clientX - getStartPosition()[0],
        y: event.clientY - getStartPosition()[1],
      },
    );
    setStartPosition([event.clientX, event.clientY]);
    for (const key of XSta.keys()) {
      if (
        key.includes('useEdgeDraggable-') &&
        !(key.endsWith('-' + dragId) || key.endsWith('-' + dragFrom))
      ) {
        XSta.delete(key);
      }
    }
  };

  useEffect(() => {
    document.addEventListener('mousedown', onDragStart);
    document.addEventListener('mousemove', onDragging);
    document.addEventListener('mouseup', onDragEnd);
    return () => {
      document.removeEventListener('mousedown', onDragStart);
      document.removeEventListener('mousemove', onDragging);
      document.removeEventListener('mouseup', onDragEnd);
    };
  }, []);

  return { dragRef, isDragging: getIsDragging() };
};

function setGlobalCursorStyle(
  cursorStyle: 'auto' | 'row-resize' | 'col-resize',
) {
  const oldStyle = document.getElementById('global-cursor-style');
  if (oldStyle) oldStyle.remove();
  if (cursorStyle !== 'auto') {
    const style = document.createElement('style');
    style.id = 'global-cursor-style';
    style.innerHTML = `* { cursor: ${cursorStyle} !important; }`;
    document.head.appendChild(style);
  }
}
