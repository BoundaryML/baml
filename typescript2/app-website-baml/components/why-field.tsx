/** biome-ignore-all lint/style/noNonNullAssertion: the canvas ref is attached by this component and 2d contexts are always available */
'use client';

import { useEffect, useRef } from 'react';

/* The backdrop under "Why a new language?": a faint field of mono glyphs that starts as noise and
   settles, once, into order — a dot grid with the hero's Verdict type set into it. It is the
   section's claim drawn as texture: invalid states made hard to represent.
   One canvas, drawn at ~12fps only while it is settling and on screen, then never again (except to
   repaint on a theme change or resize). Reduced motion gets the settled state straight away. Set in
   the site's mono (JetBrains Mono) in --ink at low alpha, so it follows the theme. */

const CODE = [
  'type Label = "positive" | "negative" | "neutral";',
  '',
  'class Verdict {',
  '  label: Label,',
  '  confidence: float,',
  '}',
];
const NOISE = '01{}[]<>()=:;|+-*/#%&$@~^._';
const FONT_PX = 13;
const ROW_PX = 20;
const CODE_ALPHA = 0.16; // settled code
const GRID_ALPHA = 0.1; // settled dots and the noise
const DURATION = 3200; // ms from first frame to fully settled
const TICK = 83; // ~12fps

type Cell = { target: string; settleAt: number; glyph: string };

function build(cols: number, rows: number): Cell[] {
  const cells: Cell[] = [];
  const codeW = Math.max(...CODE.map((l) => l.length));
  const top = 1; // one quiet row above the code block
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) {
      const line = CODE[r - top];
      const inCode = line !== undefined && c <= codeW + 1;
      const target = inCode
        ? (line[c] ?? ' ')
        : r % 2 === 0 && c % 3 === 0 && !(r === top + CODE.length)
          ? '·'
          : ' ';
      // Resolves broadly left to right, top to bottom, with enough jitter to read as settling.
      const settleAt =
        (c / cols) * 0.45 + (r / rows) * 0.15 + Math.random() * 0.4;
      cells.push({ glyph: ' ', settleAt, target });
    }
  }
  return cells;
}

export default function WhyField() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current!;
    const ctx = canvas.getContext('2d')!;
    const reduce = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches;
    let cols = 0;
    let rows = 0;
    let cellW = 8;
    let cells: Cell[] = [];
    let progress = reduce ? 1 : 0; // 0 → 1 across DURATION
    let visible = false;
    let raf = 0;
    let last = 0;
    let startedAt = 0;
    let ready = false;

    const ink = () =>
      getComputedStyle(canvas).getPropertyValue('--ink').trim() || '#888';
    const family = getComputedStyle(canvas).fontFamily;

    function layout() {
      const { width, height } = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.font = `${FONT_PX}px ${family}`;
      cellW = ctx.measureText('0').width;
      const nc = Math.floor(width / cellW);
      const nr = Math.floor(height / ROW_PX);
      if (nc !== cols || nr !== rows) {
        cols = nc;
        rows = nr;
        cells = build(cols, rows);
      }
    }

    function draw() {
      const { width, height } = canvas.getBoundingClientRect();
      ctx.clearRect(0, 0, width, height);
      ctx.font = `${FONT_PX}px ${family}`;
      ctx.textBaseline = 'top';
      ctx.fillStyle = ink();
      for (let i = 0; i < cells.length; i++) {
        const cell = cells[i];
        const settled = progress >= cell.settleAt;
        let ch: string;
        if (settled) ch = cell.target;
        else {
          // Unsettled cells flicker: about half are blank, and a third re-roll each tick.
          if (cell.glyph === ' ' || Math.random() < 0.35)
            cell.glyph =
              Math.random() < 0.5
                ? NOISE[(Math.random() * NOISE.length) | 0]
                : '·';
          ch = cell.glyph;
        }
        if (ch === ' ') continue;
        ctx.globalAlpha =
          settled && cell.target !== '·' ? CODE_ALPHA : GRID_ALPHA;
        ctx.fillText(ch, (i % cols) * cellW, Math.floor(i / cols) * ROW_PX + 3);
      }
      ctx.globalAlpha = 1;
    }

    function frame(t: number) {
      raf = 0;
      if (!visible || progress >= 1) return;
      if (!startedAt) startedAt = t - progress * DURATION;
      if (t - last >= TICK) {
        last = t;
        progress = Math.min(1, (t - startedAt) / DURATION);
        draw();
      }
      if (progress < 1) raf = requestAnimationFrame(frame);
    }

    function play() {
      if (!ready || raf || progress >= 1) return;
      startedAt = 0; // resume from the current progress
      raf = requestAnimationFrame(frame);
    }

    const io = new IntersectionObserver(
      ([e]) => {
        visible = e.isIntersecting;
        if (visible) play();
        else if (raf) {
          cancelAnimationFrame(raf);
          raf = 0;
        }
      },
      { threshold: 0.35 },
    );

    const ro = new ResizeObserver(() => {
      if (!ready) return;
      layout();
      if (progress >= 1 || !raf) draw();
    });
    const repaint = () => {
      if (ready && (progress >= 1 || !raf)) draw();
    };
    const mo = new MutationObserver(repaint);
    const mq = window.matchMedia('(prefers-color-scheme: dark)');

    document.fonts.ready.then(() => {
      ready = true;
      layout();
      draw(); // reduced motion: the settled field; otherwise the opening noise, waiting for view
      io.observe(canvas);
      ro.observe(canvas);
      mo.observe(document.documentElement, {
        attributeFilter: ['data-theme'],
        attributes: true,
      });
      mq.addEventListener('change', repaint);
    });

    return () => {
      if (raf) cancelAnimationFrame(raf);
      io.disconnect();
      ro.disconnect();
      mo.disconnect();
      mq.removeEventListener('change', repaint);
    };
  }, []);

  return (
    <canvas
      aria-hidden
      className="absolute inset-0 h-full w-full font-mono pointer-events-none"
      ref={ref}
    />
  );
}
