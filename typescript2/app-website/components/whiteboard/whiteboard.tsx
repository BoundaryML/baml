'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

// Experimental: the whole site becomes a whiteboard. A fixed canvas overlays
// the page for freehand ink (doc-space strokes, so drawings scroll with the
// content), a floating toolbar switches tools, text stickies can be dropped
// anywhere, and in move mode every top-level block of the page is draggable
// like a thing pinned to a board. Sketch mode adds wobbly hand-drawn framing.

type Tool = 'browse' | 'move' | 'pen' | 'hl' | 'eraser' | 'text';

type Stroke = {
  color: string;
  size: number;
  alpha: number;
  // Document-space points, so ink stays glued to the content while scrolling.
  pts: [number, number][];
};

type Sticky = {
  id: number;
  x: number;
  y: number;
  text: string;
};

const COLORS = ['#1A1612', '#6D28D9', '#B4342B', '#1F8B4C'];

let stickySeq = 1;

export function Whiteboard() {
  const [tool, setTool] = useState<Tool>('browse');
  const [color, setColor] = useState(COLORS[1]);
  const [sketchy, setSketchy] = useState(false);
  const [stickies, setStickies] = useState<Sticky[]>([]);

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const strokesRef = useRef<Stroke[]>([]);
  const liveRef = useRef<Stroke | null>(null);
  const toolRef = useRef(tool);
  toolRef.current = tool;
  const colorRef = useRef(color);
  colorRef.current = color;

  // Dragged page block (move mode) or sticky, with pointer offsets.
  const dragRef = useRef<{
    el: HTMLElement;
    startX: number;
    startY: number;
    baseX: number;
    baseY: number;
  } | null>(null);

  const redraw = useCallback(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, canvas.width / dpr, canvas.height / dpr);
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    const sy = window.scrollY;
    const paint = (s: Stroke) => {
      if (s.pts.length < 2) return;
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.size;
      ctx.globalAlpha = s.alpha;
      ctx.beginPath();
      ctx.moveTo(s.pts[0][0], s.pts[0][1] - sy);
      for (const [x, y] of s.pts.slice(1)) ctx.lineTo(x, y - sy);
      ctx.stroke();
    };
    for (const s of strokesRef.current) paint(s);
    if (liveRef.current) paint(liveRef.current);
    ctx.globalAlpha = 1;
  }, []);

  // Canvas sizing + scroll tracking.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const fit = () => {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = window.innerWidth * dpr;
      canvas.height = window.innerHeight * dpr;
      canvas.style.width = `${window.innerWidth}px`;
      canvas.style.height = `${window.innerHeight}px`;
      redraw();
    };
    fit();
    let raf = 0;
    const onScroll = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(redraw);
    };
    window.addEventListener('resize', fit);
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      window.removeEventListener('resize', fit);
      window.removeEventListener('scroll', onScroll);
      cancelAnimationFrame(raf);
    };
  }, [redraw]);

  // Sketch mode flips a body class that the CSS below keys off.
  useEffect(() => {
    document.body.classList.toggle('xp-sketchy', sketchy);
    return () => document.body.classList.remove('xp-sketchy');
  }, [sketchy]);

  // Move mode: any direct child of a <main> is a draggable board object.
  useEffect(() => {
    document.body.classList.toggle('xp-move', tool === 'move');
    if (tool !== 'move') return;
    const down = (e: PointerEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest('.xp-toolbar, .xp-sticky')) return;
      const el = t.closest('main > *') as HTMLElement | null;
      if (!el) return;
      e.preventDefault();
      const baseX = Number(el.dataset.xpX || 0);
      const baseY = Number(el.dataset.xpY || 0);
      dragRef.current = {
        baseX,
        baseY,
        el,
        startX: e.clientX,
        startY: e.clientY,
      };
      el.classList.add('xp-dragging');
    };
    const move = (e: PointerEvent) => {
      const d = dragRef.current;
      if (!d) return;
      const x = d.baseX + e.clientX - d.startX;
      const y = d.baseY + e.clientY - d.startY;
      d.el.dataset.xpX = String(x);
      d.el.dataset.xpY = String(y);
      d.el.style.transform = `translate(${x}px, ${y}px)`;
    };
    const up = () => {
      dragRef.current?.el.classList.remove('xp-dragging');
      dragRef.current = null;
    };
    document.addEventListener('pointerdown', down);
    document.addEventListener('pointermove', move);
    document.addEventListener('pointerup', up);
    return () => {
      document.removeEventListener('pointerdown', down);
      document.removeEventListener('pointermove', move);
      document.removeEventListener('pointerup', up);
      up();
    };
  }, [tool]);

  // Ink tools live on the canvas itself.
  const onCanvasDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const t = toolRef.current;
    const doc: [number, number] = [e.clientX, e.clientY + window.scrollY];
    if (t === 'text') {
      setStickies((prev) => [
        ...prev,
        { id: stickySeq++, text: '', x: doc[0], y: doc[1] },
      ]);
      setTool('browse');
      return;
    }
    if (t === 'eraser') {
      eraseAt(doc);
      liveRef.current = { alpha: 0, color: '', pts: [doc], size: 0 };
      return;
    }
    liveRef.current = {
      alpha: t === 'hl' ? 0.35 : 1,
      color: colorRef.current,
      pts: [doc],
      size: t === 'hl' ? 14 : 2.5,
    };
    (e.target as HTMLCanvasElement).setPointerCapture(e.pointerId);
  };

  const eraseAt = (p: [number, number]) => {
    const r = 14;
    const before = strokesRef.current.length;
    strokesRef.current = strokesRef.current.filter(
      (s) => !s.pts.some(([x, y]) => Math.hypot(x - p[0], y - p[1]) < r),
    );
    if (strokesRef.current.length !== before) redraw();
  };

  const onCanvasMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const live = liveRef.current;
    if (!live) return;
    const doc: [number, number] = [e.clientX, e.clientY + window.scrollY];
    if (toolRef.current === 'eraser') {
      eraseAt(doc);
      return;
    }
    live.pts.push(doc);
    redraw();
  };

  const onCanvasUp = () => {
    const live = liveRef.current;
    liveRef.current = null;
    if (live && live.pts.length > 1 && toolRef.current !== 'eraser') {
      strokesRef.current.push(live);
    }
    redraw();
  };

  const undo = () => {
    strokesRef.current.pop();
    redraw();
  };
  const clearAll = () => {
    strokesRef.current = [];
    setStickies([]);
    redraw();
  };

  // Sticky dragging works in every mode, by the note's grip strip.
  const stickyDown = (e: React.PointerEvent, id: number) => {
    const el = (e.currentTarget as HTMLElement).closest(
      '.xp-sticky',
    ) as HTMLElement;
    const startX = e.clientX;
    const startY = e.clientY;
    const s = stickies.find((n) => n.id === id);
    if (!s || !el) return;
    const move = (ev: PointerEvent) => {
      setStickies((prev) =>
        prev.map((n) =>
          n.id === id
            ? {
                ...n,
                x: s.x + ev.clientX - startX,
                y: s.y + ev.clientY - startY,
              }
            : n,
        ),
      );
    };
    const up = () => {
      document.removeEventListener('pointermove', move);
      document.removeEventListener('pointerup', up);
    };
    document.addEventListener('pointermove', move);
    document.addEventListener('pointerup', up);
  };

  const inking =
    tool === 'pen' || tool === 'hl' || tool === 'eraser' || tool === 'text';

  return (
    <>
      <canvas
        className="xp-canvas"
        onPointerDown={onCanvasDown}
        onPointerMove={onCanvasMove}
        onPointerUp={onCanvasUp}
        ref={canvasRef}
        style={{ pointerEvents: inking ? 'auto' : 'none' }}
      />

      {/* Doc-anchored layer for stickies (height 0, overflow visible). */}
      <div className="xp-notes">
        {stickies.map((s) => (
          <div className="xp-sticky" key={s.id} style={{ left: s.x, top: s.y }}>
            <button
              aria-label="Drag note"
              className="xp-sticky-grip"
              onPointerDown={(e) => stickyDown(e, s.id)}
              type="button"
            >
              &#10495;
            </button>
            <div
              className="xp-sticky-body"
              contentEditable
              // biome-ignore lint/a11y/noAutofocus: a fresh note wants a caret
              ref={(el) => {
                if (el && !s.text) el.focus();
              }}
              suppressContentEditableWarning
            >
              {s.text}
            </div>
            <button
              aria-label="Delete note"
              className="xp-sticky-x"
              onClick={() =>
                setStickies((prev) => prev.filter((n) => n.id !== s.id))
              }
              type="button"
            >
              &times;
            </button>
          </div>
        ))}
      </div>

      <div className="xp-toolbar">
        {(
          [
            ['browse', '🖱', 'Browse'],
            ['move', '✋', 'Move blocks'],
            ['pen', '✏️', 'Pen'],
            ['hl', '🖊', 'Highlighter'],
            ['eraser', '🩹', 'Eraser'],
            ['text', '💬', 'Note'],
          ] as [Tool, string, string][]
        ).map(([t, icon, label]) => (
          <button
            aria-pressed={tool === t}
            className={`xp-btn${tool === t ? ' on' : ''}`}
            key={t}
            onClick={() => setTool(t)}
            title={label}
            type="button"
          >
            {icon}
          </button>
        ))}
        <span className="xp-sep" />
        {COLORS.map((c) => (
          <button
            aria-label={`Ink ${c}`}
            aria-pressed={color === c}
            className={`xp-swatch${color === c ? ' on' : ''}`}
            key={c}
            onClick={() => setColor(c)}
            style={{ background: c }}
            type="button"
          />
        ))}
        <span className="xp-sep" />
        <button
          className="xp-btn"
          onClick={undo}
          title="Undo stroke"
          type="button"
        >
          ↩️
        </button>
        <button
          className="xp-btn"
          onClick={clearAll}
          title="Clear board"
          type="button"
        >
          🗑
        </button>
        <button
          aria-pressed={sketchy}
          className={`xp-btn${sketchy ? ' on' : ''}`}
          onClick={() => setSketchy((v) => !v)}
          title="Sketchy mode"
          type="button"
        >
          🌀
        </button>
      </div>

      <style>{`
        .xp-canvas { position: fixed; inset: 0; z-index: 45;
          cursor: crosshair; touch-action: none; }
        .xp-notes { position: absolute; top: 0; left: 0; width: 100%;
          height: 0; overflow: visible; z-index: 46; }
        .xp-sticky { position: absolute; min-width: 150px; max-width: 260px;
          background: #FEF6C7; border: 1px solid #E3D48A; border-radius: 3px;
          box-shadow: 3px 5px 0 rgba(26, 22, 18, 0.12);
          transform: rotate(-1deg); padding: 20px 22px 10px 10px; }
        .xp-sticky-body { outline: none; font-size: 14px; line-height: 1.45;
          font-family: 'Marker Felt', 'Comic Sans MS', cursive; min-height: 20px; }
        .xp-sticky-grip { position: absolute; top: 2px; left: 6px; border: 0;
          background: none; cursor: grab; color: #B8A44E; font-size: 12px;
          padding: 2px; }
        .xp-sticky-x { position: absolute; top: 0; right: 4px; border: 0;
          background: none; cursor: pointer; color: #B8A44E; font-size: 15px; }
        .xp-toolbar { position: fixed; bottom: 18px; left: 50%;
          transform: translateX(-50%); z-index: 60; display: flex; gap: 4px;
          align-items: center; padding: 7px 10px; background: #FFFDF7;
          border: 2px solid #1A1612;
          border-radius: 255px 15px 225px 15px / 15px 225px 15px 255px;
          box-shadow: 4px 6px 0 rgba(26, 22, 18, 0.15); }
        .xp-btn { border: 1px solid transparent; background: none;
          border-radius: 8px; font-size: 17px; line-height: 1; padding: 6px 8px;
          cursor: pointer; }
        .xp-btn:hover { background: rgba(109, 40, 217, 0.08); }
        .xp-btn.on { border-color: #6D28D9; background: #F3EEFE; }
        .xp-swatch { width: 18px; height: 18px; border-radius: 50%;
          border: 2px solid #FFFDF7; cursor: pointer; }
        .xp-swatch.on { outline: 2px solid #1A1612; }
        .xp-sep { width: 1px; height: 20px; background: #D9D3C4; margin: 0 4px; }
        /* move mode: blocks feel like pinned board objects */
        body.xp-move main > * { cursor: grab; }
        body.xp-move main > *:hover { outline: 2px dashed #6D28D9;
          outline-offset: 6px; }
        .xp-dragging { cursor: grabbing !important; opacity: 0.92; z-index: 40;
          position: relative; }
        /* sketchy mode: wobbly hand-drawn framing on the page's blocks */
        body.xp-sketchy main > * { border: 2px solid #1A1612;
          border-radius: 255px 15px 225px 15px / 15px 225px 15px 255px;
          padding: 18px 22px; background: #FFFDF7;
          box-shadow: 4px 6px 0 rgba(26, 22, 18, 0.08); }
        body.xp-sketchy main > *:nth-child(odd) { rotate: -0.4deg; }
        body.xp-sketchy main > *:nth-child(even) { rotate: 0.35deg; }
        body.xp-sketchy h1, body.xp-sketchy h2, body.xp-sketchy h3 {
          font-family: 'Marker Felt', 'Comic Sans MS', cursive; }
      `}</style>
    </>
  );
}
