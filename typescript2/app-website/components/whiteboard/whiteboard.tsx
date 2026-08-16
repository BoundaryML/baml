'use client';

import { usePathname } from 'next/navigation';
import { useCallback, useEffect, useRef, useState } from 'react';

// Experimental: the whole site becomes a whiteboard. A fixed canvas overlays
// the page for freehand ink (doc-space strokes, so drawings scroll with the
// content), a floating toolbar switches tools, text stickies can be dropped
// anywhere, and in move mode every top-level block of the page is draggable
// like a thing pinned to a board. Sketch mode adds wobbly hand-drawn framing.

type Tool = 'browse' | 'move' | 'pen' | 'hl' | 'eraser' | 'text' | 'label';

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
  /** note = yellow paper; label = bare text on the board */
  kind: 'note' | 'label';
};

// Board contents survive reloads.
const STORAGE_KEY = 'xp-board-v1';
// Dragged page-block offsets, keyed by pathname then block index.
const BLOCKS_KEY = 'xp-blocks-v1';
const ZOOM_KEY = 'xp-zoom-v1';
const ZOOM_MIN = 0.25;
const ZOOM_MAX = 3;

type BlockMap = Record<string, Record<string, [number, number]>>;

function loadBlocks(): BlockMap {
  try {
    return JSON.parse(localStorage.getItem(BLOCKS_KEY) || '{}');
  } catch {
    return {};
  }
}

function saveBlocks(m: BlockMap) {
  try {
    localStorage.setItem(BLOCKS_KEY, JSON.stringify(m));
  } catch {
    /* fine */
  }
}

type Action = { kind: 'stroke' } | { kind: 'sticky'; id: number };

const COLORS = ['#1A1612', '#6D28D9', '#B4342B', '#1F8B4C'];

let stickySeq = 1;

// Minimal single-stroke icons, sized by the button's font-size via em units.
function icon(d: string) {
  return (
    <svg
      aria-hidden
      fill="none"
      height="18"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.8"
      viewBox="0 0 24 24"
      width="18"
    >
      <path d={d} />
    </svg>
  );
}

const ICONS = {
  cursor: icon('M5 3l14 8-6.5 1.5L9 19z'),
  eraser: icon('M6 20h12M6 16l8-8 4 4-6 6H8z'),
  hand: icon(
    'M8 12V6.5a1.5 1.5 0 0 1 3 0V11m0-5.5a1.5 1.5 0 0 1 3 0V11m0-3.5a1.5 1.5 0 0 1 3 0V13c0 4-2.5 7-6.5 7S6 17 6 14v-3a1.5 1.5 0 0 1 2 0',
  ),
  hl: icon('M9 15l-4 4H3v-2l4-4m2 2l8-8 2 2-8 8m-4-4l4 4M14 5l3 3'),
  minus: icon('M6 12h12'),
  note: icon('M4 5h16v10H10l-4 4v-4H4z'),
  pen: icon('M4 20l1-4L16 5l3 3L8 19zM14 7l3 3'),
  plus: icon('M12 6v12M6 12h12'),
  trash: icon('M5 7h14M9 7V4h6v3m-8 0l1 13h8l1-13'),
  type: icon('M6 6h12M12 6v13M9 19h6'),
  undo: icon('M8 5L4 9l4 4M4 9h10a5 5 0 0 1 0 10h-3'),
};

const TOOL_BUTTONS: [Tool, JSX.Element, string][] = [
  ['browse', ICONS.cursor, 'Browse'],
  ['move', ICONS.hand, 'Move blocks'],
  ['pen', ICONS.pen, 'Pen'],
  ['hl', ICONS.hl, 'Highlighter'],
  ['eraser', ICONS.eraser, 'Eraser'],
  ['text', ICONS.note, 'Note'],
  ['label', ICONS.type, 'Text'],
];

export function Whiteboard() {
  const pathname = usePathname();
  const [tool, setTool] = useState<Tool>('browse');
  const [color, setColor] = useState(COLORS[1]);
  const [zoom, setZoom] = useState(1);
  const zoomRef = useRef(1);
  zoomRef.current = zoom;
  const [stickies, setStickies] = useState<Sticky[]>([]);

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const strokesRef = useRef<Stroke[]>([]);
  const liveRef = useRef<Stroke | null>(null);
  const actionsRef = useRef<Action[]>([]);
  const stickiesRef = useRef<Sticky[]>([]);
  stickiesRef.current = stickies;

  const save = useCallback(() => {
    try {
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify({
          stickies: stickiesRef.current,
          strokes: strokesRef.current,
        }),
      );
    } catch {
      /* storage full or unavailable */
    }
  }, []);
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
    // Strokes live in layout space; project through the current zoom.
    const z = zoomRef.current;
    const sx = window.scrollX;
    const sy = window.scrollY;
    const paint = (s: Stroke) => {
      if (s.pts.length < 2) return;
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.size * z;
      ctx.globalAlpha = s.alpha;
      ctx.beginPath();
      ctx.moveTo(s.pts[0][0] * z - sx, s.pts[0][1] * z - sy);
      for (const [x, y] of s.pts.slice(1)) ctx.lineTo(x * z - sx, y * z - sy);
      ctx.stroke();
    };
    for (const s of strokesRef.current) paint(s);
    if (liveRef.current) paint(liveRef.current);
    ctx.globalAlpha = 1;
  }, []);

  // Zoom scales the page itself (css zoom); the fixed overlays counter-zoom
  // so the toolbar and canvas stay viewport-true. Cmd/Ctrl+wheel also zooms.
  useEffect(() => {
    document.body.style.zoom = String(zoom);
    redraw();
    try {
      localStorage.setItem(ZOOM_KEY, String(zoom));
    } catch {
      /* fine */
    }
  }, [zoom, redraw]);

  useEffect(() => {
    const onWheel = (e: WheelEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      e.preventDefault();
      setZoom((z) =>
        Math.min(
          ZOOM_MAX,
          Math.max(ZOOM_MIN, z * (e.deltaY < 0 ? 1.06 : 0.94)),
        ),
      );
    };
    window.addEventListener('wheel', onWheel, { passive: false });
    return () => window.removeEventListener('wheel', onWheel);
  }, []);

  // Re-apply persisted block drags for this route (content mounts async).
  useEffect(() => {
    const apply = () => {
      const saved = loadBlocks()[pathname || '/'] || {};
      const blocks = document.querySelectorAll('main > *');
      blocks.forEach((el, i) => {
        const pos = saved[String(i)];
        if (!pos) return;
        const h = el as HTMLElement;
        h.dataset.xpX = String(pos[0]);
        h.dataset.xpY = String(pos[1]);
        h.style.transform = `translate(${pos[0]}px, ${pos[1]}px)`;
      });
    };
    const t = setTimeout(apply, 60);
    return () => clearTimeout(t);
  }, [pathname]);

  // Restore the persisted board once on mount.
  useEffect(() => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return;
      const data = JSON.parse(raw) as {
        strokes?: Stroke[];
        stickies?: Sticky[];
      };
      strokesRef.current = data.strokes ?? [];
      const z = Number(localStorage.getItem(ZOOM_KEY));
      if (z >= ZOOM_MIN && z <= ZOOM_MAX) setZoom(z);
      const notes = (data.stickies ?? []).map((n) => ({
        ...n,
        kind: n.kind ?? ('note' as const),
      }));
      setStickies(notes);
      stickySeq = Math.max(0, ...notes.map((n) => n.id)) + 1;
      redraw();
    } catch {
      /* corrupt board state: start fresh */
    }
  }, [redraw]);

  // Stickies persist whenever they change (position, text, add, delete).
  useEffect(() => {
    save();
  }, [stickies, save]);

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
      const d = dragRef.current;
      if (d) {
        d.el.classList.remove('xp-dragging');
        // Persist this block's offset under its route + index.
        const blocks = Array.from(document.querySelectorAll('main > *'));
        const idx = blocks.indexOf(d.el);
        if (idx >= 0) {
          const all = loadBlocks();
          const page = all[pathname || '/'] || {};
          page[String(idx)] = [
            Number(d.el.dataset.xpX || 0),
            Number(d.el.dataset.xpY || 0),
          ];
          all[pathname || '/'] = page;
          saveBlocks(all);
        }
      }
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
  }, [tool, pathname]);

  // Ink tools live on the canvas itself.
  const onCanvasDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const t = toolRef.current;
    const z = zoomRef.current;
    const doc: [number, number] = [
      (e.clientX + window.scrollX) / z,
      (e.clientY + window.scrollY) / z,
    ];
    if (t === 'text' || t === 'label') {
      e.preventDefault();
      const id = stickySeq++;
      actionsRef.current.push({ id, kind: 'sticky' });
      setStickies((prev) => [
        ...prev,
        {
          id,
          kind: t === 'text' ? 'note' : 'label',
          text: '',
          x: doc[0],
          y: doc[1],
        },
      ]);
      // Switch back to browse only after this pointer's click has fully
      // dispatched, so the click cannot fall through onto the page and yank
      // focus away from the new note.
      setTimeout(() => setTool('browse'), 0);
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
    const r = 14 / zoomRef.current;
    const before = strokesRef.current.length;
    strokesRef.current = strokesRef.current.filter(
      (s) => !s.pts.some(([x, y]) => Math.hypot(x - p[0], y - p[1]) < r),
    );
    if (strokesRef.current.length !== before) redraw();
  };

  const onCanvasMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const live = liveRef.current;
    if (!live) return;
    const z = zoomRef.current;
    const doc: [number, number] = [
      (e.clientX + window.scrollX) / z,
      (e.clientY + window.scrollY) / z,
    ];
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
      actionsRef.current.push({ kind: 'stroke' });
    }
    redraw();
    save();
  };

  // Undo walks one shared history of stroke and sticky additions.
  const undo = useCallback(() => {
    const action = actionsRef.current.pop();
    if (!action) return;
    if (action.kind === 'stroke') {
      strokesRef.current.pop();
      redraw();
      save();
    } else {
      setStickies((prev) => prev.filter((n) => n.id !== action.id));
    }
  }, [redraw, save]);

  const clearAll = () => {
    if (
      !window.confirm(
        'Clear the whole board? Ink, notes, and moved blocks reset.',
      )
    )
      return;
    strokesRef.current = [];
    actionsRef.current = [];
    setStickies([]);
    for (const el of document.querySelectorAll('main > *')) {
      const h = el as HTMLElement;
      if (h.dataset.xpX || h.dataset.xpY) {
        h.style.transform = '';
        delete h.dataset.xpX;
        delete h.dataset.xpY;
      }
    }
    try {
      localStorage.removeItem(STORAGE_KEY);
      localStorage.removeItem(BLOCKS_KEY);
      localStorage.removeItem(ZOOM_KEY);
    } catch {
      /* fine */
    }
    setZoom(1);
    redraw();
  };

  // Cmd/Ctrl+Z undoes board edits, except while typing in a note (there the
  // browser's own text undo should win).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        !(e.metaKey || e.ctrlKey) ||
        e.key.toLowerCase() !== 'z' ||
        e.shiftKey
      )
        return;
      const a = document.activeElement as HTMLElement | null;
      if (
        a &&
        (a.isContentEditable ||
          a.tagName === 'INPUT' ||
          a.tagName === 'TEXTAREA')
      )
        return;
      e.preventDefault();
      undo();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [undo]);

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
                x: s.x + (ev.clientX - startX) / zoomRef.current,
                y: s.y + (ev.clientY - startY) / zoomRef.current,
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
    tool === 'pen' ||
    tool === 'hl' ||
    tool === 'eraser' ||
    tool === 'text' ||
    tool === 'label';

  return (
    <>
      <canvas
        className="xp-canvas"
        data-zoom={zoom}
        onPointerDown={onCanvasDown}
        onPointerMove={onCanvasMove}
        onPointerUp={onCanvasUp}
        ref={canvasRef}
        style={{
          pointerEvents: inking ? 'auto' : 'none',
          zoom: 1 / zoom,
        }}
      />

      {/* Doc-anchored layer for stickies (height 0, overflow visible). */}
      <div className="xp-notes">
        {stickies.map((s) => (
          <div
            className={`xp-sticky${s.kind === 'label' ? ' xp-sticky--label' : ''}`}
            key={s.id}
            style={{ left: s.x, top: s.y }}
          >
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
              onInput={(e) => {
                const text = (e.currentTarget as HTMLElement).innerHTML;
                setStickies((prev) =>
                  prev.map((n) => (n.id === s.id ? { ...n, text } : n)),
                );
              }}
              // Seed the content once and never re-render it: React owns no
              // children here, so typing and re-renders cannot fight. Stored
              // as HTML so Cmd+B / Cmd+I formatting persists.
              ref={(el) => {
                if (el && el.dataset.init !== '1') {
                  el.dataset.init = '1';
                  el.innerHTML = s.text;
                  if (!s.text) {
                    requestAnimationFrame(() => {
                      el.focus();
                      const sel = window.getSelection();
                      sel?.selectAllChildren(el);
                      sel?.collapseToEnd();
                    });
                  }
                }
              }}
              suppressContentEditableWarning
            />
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

      <div className="xp-toolbar" style={{ zoom: 1 / zoom }}>
        {TOOL_BUTTONS.map(([t, icon, label]) => (
          <button
            aria-label={label}
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
          {ICONS.undo}
        </button>
        <button
          className="xp-btn"
          onClick={clearAll}
          title="Clear board"
          type="button"
        >
          {ICONS.trash}
        </button>
      </div>

      <style>{`
        /* whiteboard grid under everything */
        body { background-color: #FBF8F0 !important;
          background-image:
            linear-gradient(rgba(26, 22, 18, 0.055) 1px, transparent 1px),
            linear-gradient(90deg, rgba(26, 22, 18, 0.055) 1px, transparent 1px) !important;
          background-size: 32px 32px !important; }
        .xp-canvas { position: fixed; inset: 0; z-index: 45;
          cursor: crosshair; touch-action: none; }
        .xp-notes { position: absolute; top: 0; left: 0; width: 100%;
          height: 0; overflow: visible; z-index: 46; }
        .xp-sticky { position: absolute; min-width: 150px; max-width: 260px;
          background: #FEF6C7; border: 1px solid #E3D48A; border-radius: 3px;
          box-shadow: 3px 5px 0 rgba(26, 22, 18, 0.12);
          transform: rotate(-1deg); padding: 20px 22px 10px 10px; }
        .xp-sticky--label { background: none; border: none; box-shadow: none;
          transform: none; padding: 14px 22px 6px 10px; min-width: 60px; }
        .xp-sticky--label .xp-sticky-body { font-size: 20px; }
        .xp-sticky--label .xp-sticky-grip,
        .xp-sticky--label .xp-sticky-x { color: rgba(26, 22, 18, 0.35); }
        .xp-sticky-body { outline: none; font-size: 14px; line-height: 1.5;
          font-family: var(--font-geist-sans), ui-sans-serif, system-ui,
            sans-serif;
          font-weight: 350; color: rgba(26, 22, 18, 0.82);
          min-height: 20px; min-width: 80px; cursor: text;
          white-space: pre-wrap; }
        .xp-sticky-body b, .xp-sticky-body strong { font-weight: 650; }
        .xp-sticky-body i, .xp-sticky-body em { font-style: italic; }
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
          border-radius: 8px; line-height: 0; padding: 7px; color: #1A1612;
          cursor: pointer; }
        .xp-btn:hover { background: rgba(109, 40, 217, 0.08); }
        .xp-btn.on { border-color: #6D28D9; background: #F3EEFE; }
        .xp-swatch { width: 18px; height: 18px; border-radius: 50%;
          border: 2px solid #FFFDF7; cursor: pointer; }
        .xp-swatch.on { outline: 2px solid #1A1612; }
        .xp-sep { width: 1px; height: 20px; background: #D9D3C4; margin: 0 4px; }
        .xp-zoom { border: 0; background: none; cursor: pointer;
          font-family: var(--font-geist-mono), ui-monospace, monospace;
          font-size: 11.5px; color: #5C5852; min-width: 40px; padding: 6px 2px; }
        .xp-zoom:hover { color: #6D28D9; }
        .xp-clear { display: inline-flex; align-items: center; gap: 6px;
          border: 1px solid #B4342B; color: #B4342B; background: #FDF0EE;
          border-radius: 8px; padding: 6px 11px; cursor: pointer;
          font-size: 13px; font-weight: 600; line-height: 1;
          font-family: inherit; }
        .xp-clear:hover { background: #B4342B; color: #fff; }
        /* move mode: blocks feel like pinned board objects */
        body.xp-move main > * { cursor: grab; }
        body.xp-move main > *:hover { outline: 2px dashed #6D28D9;
          outline-offset: 6px; }
        .xp-dragging { cursor: grabbing !important; opacity: 0.92; z-index: 40;
          position: relative; }
      `}</style>
    </>
  );
}
