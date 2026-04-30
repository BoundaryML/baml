'use client';

import { useEffect, useRef, useState } from 'react';

interface EditorTerminalSplitProps {
  editor: React.ReactNode;
  terminal: React.ReactNode;
  initialTerminalHeight?: number;
  minTerminalHeight?: number;
  minEditorHeight?: number;
}

export function EditorTerminalSplit({
  editor,
  terminal,
  initialTerminalHeight = 200,
  minTerminalHeight = 60,
  minEditorHeight = 180,
}: EditorTerminalSplitProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [terminalHeight, setTerminalHeight] = useState(initialTerminalHeight);
  const [dragging, setDragging] = useState(false);
  const draggingRef = useRef(false);

  const beginDrag = () => {
    draggingRef.current = true;
    setDragging(true);
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
  };

  const endDrag = () => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    setDragging(false);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  };

  const updateFromY = (clientY: number) => {
    const c = containerRef.current;
    if (!c) return;
    const rect = c.getBoundingClientRect();
    const proposed = rect.bottom - clientY;
    const max = Math.max(minTerminalHeight, rect.height - minEditorHeight);
    const clamped = Math.max(minTerminalHeight, Math.min(max, proposed));
    setTerminalHeight(clamped);
  };

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!draggingRef.current) return;
      e.preventDefault();
      updateFromY(e.clientY);
    };
    const onTouchMove = (e: TouchEvent) => {
      if (!draggingRef.current) return;
      const t = e.touches[0];
      if (!t) return;
      updateFromY(t.clientY);
    };
    const onUp = () => endDrag();
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('touchmove', onTouchMove, { passive: true });
    window.addEventListener('touchend', onUp);
    window.addEventListener('touchcancel', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      window.removeEventListener('touchmove', onTouchMove);
      window.removeEventListener('touchend', onUp);
      window.removeEventListener('touchcancel', onUp);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [minTerminalHeight, minEditorHeight]);

  const onDoubleClick = () => setTerminalHeight(initialTerminalHeight);

  return (
    <div
      ref={containerRef}
      style={{
        display: 'flex',
        flex: 1,
        flexDirection: 'column',
        minHeight: 0,
      }}
    >
      <div style={{ flex: 1, minHeight: 0, overflow: 'hidden' }}>{editor}</div>
      <div
        aria-label="Resize terminal: drag to adjust, double-click to reset"
        aria-orientation="horizontal"
        onDoubleClick={onDoubleClick}
        onMouseDown={(e) => {
          e.preventDefault();
          beginDrag();
        }}
        onTouchStart={() => beginDrag()}
        role="separator"
        style={{
          alignItems: 'center',
          background: dragging ? '#C9C3FF' : '#0E0B14',
          borderBottom: '1px solid rgba(255,255,255,0.08)',
          borderTop: '1px solid #D9D3C4',
          cursor: 'row-resize',
          display: 'flex',
          flexShrink: 0,
          height: 8,
          justifyContent: 'center',
          touchAction: 'none',
          transition: dragging ? 'none' : 'background-color 120ms ease',
        }}
      >
        <div
          style={{
            background: dragging ? '#0E0B14' : 'rgba(232,227,219,0.45)',
            borderRadius: 1,
            height: 2,
            width: 36,
          }}
        />
      </div>
      <div style={{ flexShrink: 0, height: terminalHeight, minHeight: 0 }}>
        {terminal}
      </div>
    </div>
  );
}
