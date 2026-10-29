/** biome-ignore-all lint/style/noNonNullAssertion: each ref is attached by the component that reads it in its effect */
/** biome-ignore-all lint/suspicious/noArrayIndexKey: the log lines are a fixed-length window keyed by slot */
'use client';

import Image from 'next/image';
import { type ReactNode, useEffect, useRef, useState } from 'react';

const reduced = () =>
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/* ---- hero log wall ------------------------------------------------------------------- */

const FNS = [
  'user.ScoreLead',
  'user.ParseReceipt',
  'user.ValidateVatId',
  'user.ProcessInvoice',
  'user.RouteEmail',
  'user.ClassifyTicket',
  'user.SummarizeCall',
  'user.ExtractInvoice',
];
const ID_CHARS = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

function nextLine(clock: Date) {
  const pick = <T,>(a: T[] | string) => a[Math.floor(Math.random() * a.length)];
  const id = () =>
    `baml_id_1_${Array.from({ length: 20 }, () => pick(ID_CHARS)).join('')}`;
  const ms = () => `${Math.round(300 + Math.random() * 2000)}ms`.padStart(6);
  const t =
    clock.toTimeString().slice(0, 8) +
    '.' +
    String(clock.getMilliseconds()).padStart(3, '0');
  return `${t.padEnd(14)}${pick(FNS).padEnd(22)}${id().padEnd(33)}${ms()}  ok`;
}

/**
 * The faint wall of captured calls behind the hero headline. Starts from the static lines
 * (so the server render matches), then a new call lands at the top every few seconds.
 */
export function LogTicker({
  initial,
  className,
}: {
  initial: string[];
  className: string;
}) {
  const [lines, setLines] = useState(initial);
  const [tick, setTick] = useState(0);
  const first = useRef(initial[0]);

  useEffect(() => {
    if (reduced()) return;
    // carry on from the newest call already on the wall (e.g. 06:59:58.411)
    const [h, m, sec] = first.current.slice(0, 12).split(':').map(Number);
    let clock = new Date(
      2026,
      0,
      1,
      h,
      m,
      Math.floor(sec),
      Math.round((sec % 1) * 1000),
    );
    const id = setInterval(() => {
      if (document.hidden) return;
      clock = new Date(clock.getTime() + 1000 + Math.random() * 1500);
      setLines((ls) => [
        nextLine(clock) +
          '          ' +
          nextLine(new Date(clock.getTime() - 900)),
        ...ls.slice(0, -1),
      ]);
      setTick((t) => t + 1);
    }, 2600);
    return () => clearInterval(id);
  }, []);

  return (
    <pre aria-hidden className={className}>
      <span className={tick ? 'block log-shift' : 'block'} key={tick}>
        {lines.map((l, i) => (
          <span
            className={i === 0 && tick ? 'block log-in' : 'block'}
            key={i === 0 ? `new-${tick}` : `l-${tick}-${i}`}
          >
            {l || ' '}
          </span>
        ))}
      </span>
    </pre>
  );
}

/* ---- painting parallax ------------------------------------------------------------- */

/**
 * A fill image that drifts a little slower than the page. The image is overscanned so the
 * drift never shows an edge.
 */
export function ParallaxImage({
  src,
  sizes,
  className = '',
  strength = 0.08,
  overscan = 12,
  quality,
  priority,
}: {
  src: string;
  sizes: string;
  className?: string;
  strength?: number;
  /** % of extra image above and below; only needs to cover the largest drift */
  overscan?: number;
  quality?: number;
  priority?: boolean;
}) {
  const wrap = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = wrap.current!;
    if (reduced()) return;
    let raf = 0;
    const update = () => {
      raf = 0;
      const r = el.parentElement!.getBoundingClientRect();
      if (r.bottom < 0 || r.top > window.innerHeight) return;
      const offset =
        (r.top + r.height / 2 - window.innerHeight / 2) * -strength;
      el.style.transform = `translate3d(0, ${offset.toFixed(1)}px, 0)`;
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(update);
    };
    update();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, [strength]);

  return (
    <div
      className="absolute inset-x-0 will-change-transform"
      ref={wrap}
      style={{ bottom: `-${overscan}%`, top: `-${overscan}%` }}
    >
      <Image
        alt=""
        className={`object-cover ${className}`}
        fill
        priority={priority}
        quality={quality}
        sizes={sizes}
        src={src}
      />
    </div>
  );
}

/* ---- play once on screen ------------------------------------------------------------- */

/**
 * Sets data-in on its element the first time it is 30% on screen; CSS does the rest.
 * `light` keeps the cards inside light in dark mode (for panels that sit on a painting).
 */
export function InView({
  children,
  className,
  light,
}: {
  children: ReactNode;
  className?: string;
  light?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [seen, setSeen] = useState(false);

  useEffect(() => {
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          setSeen(true);
          io.disconnect();
        }
      },
      { threshold: 0.15 },
    );
    io.observe(ref.current!);
    return () => io.disconnect();
  }, []);

  return (
    <div
      className={className}
      data-in={seen ? '' : undefined}
      data-theme={light ? 'light' : undefined}
      ref={ref}
    >
      {children}
    </div>
  );
}

/* ---- headlines rise in by line ----------------------------------------------------- */

/**
 * Splits a headline into words, works out which line each word landed on, and lets CSS lift
 * the lines in one after another the first time the headline is on screen.
 */
export function Rise({
  text,
  as: Tag = 'h2',
  className,
}: {
  text: string;
  as?: 'h2' | 'h3' | 'p' | 'blockquote';
  className?: string;
}) {
  const ref = useRef<HTMLElement>(null);
  const [seen, setSeen] = useState(false);

  useEffect(() => {
    const el = ref.current!;
    const mark = () => {
      const words = [...el.querySelectorAll<HTMLSpanElement>('.rise-word')];
      const tops = [...new Set(words.map((w) => w.offsetTop))].sort(
        (a, b) => a - b,
      );
      words.forEach((w) =>
        w.style.setProperty('--line', String(tops.indexOf(w.offsetTop))),
      );
    };
    mark();
    window.addEventListener('resize', mark);
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          setSeen(true);
          io.disconnect();
        }
      },
      { threshold: 0.4 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      window.removeEventListener('resize', mark);
    };
  }, []);

  const words = text.split(' ');
  return (
    <Tag
      className={className}
      data-in={seen ? '' : undefined}
      ref={ref as never}
    >
      {words.map((w, i) => (
        <span key={i}>
          <span className="rise-word">{w}</span>
          {i < words.length - 1 ? ' ' : ''}
        </span>
      ))}
    </Tag>
  );
}

/* ---- stats resolve digit by digit -------------------------------------------------- */

const DIGITS = '0123456789';

/**
 * A stat that scrambles and then settles one character at a time, left to right, when it
 * first comes on screen. Works the same for the placeholder "00×" and real numbers.
 */
export function Resolve({
  value,
  className,
}: {
  value: string;
  className?: string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const [shown, setShown] = useState(value);

  useEffect(() => {
    if (reduced()) return;
    const el = ref.current!;
    let timer: ReturnType<typeof setInterval> | undefined;
    const io = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting) return;
        io.disconnect();
        const start = performance.now();
        timer = setInterval(() => {
          const t = performance.now() - start;
          const settled = Math.floor((t - 250) / 110); // one more character locks every 110ms
          const out = [...value]
            .map((ch, i) =>
              i < settled || !/\d/.test(ch)
                ? ch
                : DIGITS[Math.floor(Math.random() * 10)],
            )
            .join('');
          setShown(out);
          if (settled >= value.length) {
            clearInterval(timer);
            setShown(value);
          }
        }, 45);
      },
      { threshold: 0.6 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      clearInterval(timer);
    };
  }, [value]);

  return (
    <span className={`tabular-nums ${className ?? ''}`} ref={ref}>
      {shown}
    </span>
  );
}

/* ---- pixel words scramble on hover ------------------------------------------------- */

const GLYPHS = 'abcdefghijklmnopqrstuvwxyz#%&*+=/<>';

/** A Geist Pixel word that briefly scrambles through glyphs when hovered, then settles. */
export function PixelWord({
  word,
  className,
}: {
  word: string;
  className?: string;
}) {
  const [shown, setShown] = useState(word);
  const timer = useRef<ReturnType<typeof setInterval> | undefined>(undefined);

  const play = () => {
    if (timer.current || reduced()) return;
    const start = performance.now();
    timer.current = setInterval(() => {
      const settled = Math.floor((performance.now() - start) / 55);
      setShown(
        [...word]
          .map((ch, i) =>
            i < settled || ch === '.'
              ? ch
              : GLYPHS[Math.floor(Math.random() * GLYPHS.length)],
          )
          .join(''),
      );
      if (settled >= word.length) {
        clearInterval(timer.current);
        timer.current = undefined;
        setShown(word);
      }
    }, 40);
  };
  useEffect(() => () => clearInterval(timer.current), []);

  return (
    <span
      className={`relative inline-block font-pixel ${className ?? ''}`}
      onMouseEnter={play}
    >
      {/* the real word holds the width, so the scramble never shifts the headline */}
      <span className={shown === word ? '' : 'invisible'}>{word}</span>
      {shown !== word && (
        <span aria-hidden className="absolute left-0 top-0 whitespace-nowrap">
          {shown}
        </span>
      )}
    </span>
  );
}
