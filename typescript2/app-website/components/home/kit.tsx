'use client';

import lamb from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import {
  type CSSProperties,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';

// Shared parts for the /diagram-styles board: the frame every diagram sits in, cards, status icons, and
// the beam that runs along a wire. Every color, border, radius, and font comes from the theme the diagram
// sits in (the .ds-theme-* classes in styles.css), so one diagram can be drawn in any style.
export const RED = 'var(--ds-bad)';
export const GREEN = 'var(--ds-ok)';

/** Tracks an element's size, for drawing wires in its own pixel space. */
export function useBox<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [box, setBox] = useState({ h: 0, w: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setBox({ h: el.clientHeight, w: el.clientWidth });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, box] as const;
}

/** A counter that steps through a cycle of n states, every ms milliseconds. Holds at last under reduced motion. */
export function useCycle(n: number, ms: number, last = n - 1) {
  const [i, setI] = useState(0);
  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setI(last);
      return;
    }
    const id = setInterval(() => setI((x) => (x + 1) % n), ms);
    return () => clearInterval(id);
  }, [n, ms, last]);
  return i;
}

/** The surface a diagram sits on. Its breakpoints are container queries, so it lays out by its own width. */
export function Frame({
  children,
  className = '',
  label,
}: {
  children: ReactNode;
  className?: string;
  label: string;
}) {
  return (
    <figure
      aria-label={label}
      className={`ds-frame @container relative overflow-hidden ${className}`}
    >
      <div
        aria-hidden
        className="ds-pattern absolute inset-0 pointer-events-none"
      />
      <div className="relative">{children}</div>
    </figure>
  );
}

/** A raised panel inside a frame. A lit card is the one the diagram is pointing at. */
export function Card({
  children,
  className = '',
  lit,
  style,
}: {
  children: ReactNode;
  className?: string;
  lit?: boolean;
  style?: CSSProperties;
}) {
  return (
    <div
      className={`ds-card ${className}`}
      data-lit={lit || undefined}
      style={style}
    >
      {children}
    </div>
  );
}

/** A beam along a path: three layers that make a bright head and a fading tail. */
export function Beam({
  color = 'var(--ds-accent)',
  d,
  delay = 0,
  dur = 3,
}: {
  color?: string;
  d: string;
  delay?: number;
  dur?: number;
}) {
  const layers = [
    { len: 0.3, opacity: 0.18, width: 1.5 },
    { len: 0.14, opacity: 0.45, width: 1.5 },
    { len: 0.04, opacity: 1, width: 2 },
  ];
  return (
    <g>
      {layers.map((l) => (
        <path
          className="ds-beam"
          d={d}
          fill="none"
          key={l.len}
          pathLength={1}
          stroke={color}
          strokeLinecap="round"
          strokeOpacity={l.opacity}
          strokeWidth={l.width}
          style={
            {
              '--delay': `${delay}s`,
              '--dur': `${dur}s`,
              '--len': l.len,
              '--max': 0.3,
            } as CSSProperties
          }
        />
      ))}
    </g>
  );
}

/** A horizontal S-curve between two points, the usual wire between two cards. */
export function wire(x1: number, y1: number, x2: number, y2: number) {
  const mid = (x1 + x2) / 2;
  return `M${x1} ${y1}C${mid} ${y1} ${mid} ${y2} ${x2} ${y2}`;
}

export function Check({ className = '' }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={`size-4 shrink-0 ${className}`}
      fill="none"
      viewBox="0 0 16 16"
    >
      <circle cx="8" cy="8" fill="var(--ds-ok)" fillOpacity="0.14" r="7.5" />
      <path
        d="M5 8.2l2 2 4-4.4"
        stroke="var(--ds-ok)"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.5"
      />
    </svg>
  );
}

export function Cross({ className = '' }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={`size-4 shrink-0 ${className}`}
      fill="none"
      viewBox="0 0 16 16"
    >
      <circle cx="8" cy="8" fill={RED} fillOpacity="0.14" r="7.5" />
      <path
        d="M5.5 5.5l5 5m0-5l-5 5"
        stroke={RED}
        strokeLinecap="round"
        strokeWidth="1.5"
      />
    </svg>
  );
}

export function Spinner({ className = '' }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={`ds-spin size-4 shrink-0 ${className}`}
      fill="none"
      viewBox="0 0 16 16"
    >
      <circle
        cx="8"
        cy="8"
        r="6"
        stroke="rgb(from var(--ds-ink) r g b / 0.15)"
        strokeWidth="1.5"
      />
      <path
        d="M8 2a6 6 0 0 1 6 6"
        stroke="var(--ds-accent)"
        strokeLinecap="round"
        strokeWidth="1.5"
      />
    </svg>
  );
}

export function Pending({ className = '' }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={`size-4 shrink-0 ${className}`}
      fill="none"
      viewBox="0 0 16 16"
    >
      <circle
        cx="8"
        cy="8"
        r="6"
        stroke="rgb(from var(--ds-ink) r g b / 0.2)"
        strokeDasharray="2 2.7"
        strokeWidth="1.5"
      />
    </svg>
  );
}

export function Lamb({ size = 28 }: { size?: number }) {
  return (
    <Image
      alt="BAML"
      className="ds-logo"
      height={size}
      src={lamb}
      width={size}
    />
  );
}

/** A small rounded label. */
export function Chip({
  children,
  className = '',
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={`inline-flex items-center gap-1.5 h-6 px-2 rounded-md border border-(--ds-line) bg-(--ds-ink)/[0.03] text-[12px] text-(--ds-ink)/70 whitespace-nowrap ${className}`}
    >
      {children}
    </span>
  );
}
