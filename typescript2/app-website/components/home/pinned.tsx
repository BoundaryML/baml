'use client';

import {
  type ReactNode,
  type RefObject,
  useEffect,
  useRef,
  useState,
} from 'react';
// Shared by the homepage's animated figures.

/** The word, in the color it always has. */
export function Hyper({
  children = 'hyperobservability',
}: {
  children?: string;
}) {
  // `hyper` sets "hyperobservability" and "hyperobservable" in the pixel font inside any heading
  // (home.css); in body text, and for any other word, it keeps the text's own font.
  const pixel = /^hyperobserv/i.test(children);
  return (
    <span className={`${pixel ? 'hyper ' : ''}text-clover`}>{children}</span>
  );
}

/** Whether the pointer left `e.currentTarget` through its top or bottom edge. A figure the reader
 *  paused stays paused when the mouse drifts out sideways, into the margin beside it; it resumes
 *  only when they move on up or down the page (or scroll it away). */
export function leftVertically(e: { clientY: number; currentTarget: Element }) {
  const r = e.currentTarget.getBoundingClientRect();
  return e.clientY <= r.top + 1 || e.clientY >= r.bottom - 1;
}

/** Progress as a straight line, with a little held at each end. */
export const sweep = (p: number) => Math.min(1, Math.max(0, (p - 0.04) / 0.9));

// ---------- figures that play on their own ----------

/** Quadratic ease in and out over 0 to 1. */
const ease = (k: number) => (k < 0.5 ? 2 * k * k : 1 - (-2 * k + 2) ** 2 / 2);

/** Whether the reader asked for less motion. */
/** Whether the visitor asked for less motion, kept current if they change it. */
export function useReducedMotion() {
  const [reduce, setReduce] = useState(false);
  useEffect(() => {
    const query = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setReduce(query.matches);
    update();
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  }, []);
  return reduce;
}

/** Turns true the first time the element's top passes `line`, a fraction of the way down the window
 *  (0.75 by default), and stays true. */
export function useInView(ref: RefObject<HTMLElement | null>, line = 0.75) {
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || seen) return;
    let io: IntersectionObserver | undefined;
    // The margin is in pixels of the window's height: browsers have disagreed on what a percentage
    // margin is relative to. A resize before the figure is seen moves the line with the window.
    const watch = () => {
      io?.disconnect();
      io = new IntersectionObserver(
        ([entry]) => {
          if (entry.isIntersecting) {
            setSeen(true);
            io?.disconnect();
          }
        },
        {
          rootMargin: `0px 0px -${Math.round((1 - line) * window.innerHeight)}px 0px`,
        },
      );
      io.observe(el);
    };
    watch();
    window.addEventListener('resize', watch);
    return () => {
      window.removeEventListener('resize', watch);
      io?.disconnect();
    };
  }, [ref, line, seen]);
  return seen;
}

/** Whether the element is on screen right now. Unlike `useInView` it keeps watching, so a loop gated
 *  on it pauses when the reader scrolls away and resumes when they come back. */
export function useVisible(ref: RefObject<HTMLElement | null>) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(
      ([entry]) => setVisible(entry.isIntersecting),
      { rootMargin: '0px 0px -25% 0px' },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [ref]);
  return visible;
}

/** A value that eases to `target` over `ms` whenever the target changes, or moves at a steady pace
 *  when `linear` is set. Under reduced motion it jumps. */
export function useTween(
  target: number,
  ms: number,
  snap = false,
  linear = false,
) {
  const reduce = useReducedMotion();
  const [value, setValue] = useState(target);
  const current = useRef(target);
  useEffect(() => {
    const from = current.current;
    if (from === target) return;
    if (reduce || snap) {
      current.current = target;
      setValue(target);
      return;
    }
    const start = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      const k = Math.min(1, (now - start) / ms);
      current.current = from + (target - from) * (linear ? k : ease(k));
      setValue(current.current);
      if (k < 1) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [target, ms, reduce, snap, linear]);
  return value;
}

/** A figure that plays from start to finish once, when it first comes into view, and then stays. Under
 *  reduced motion it shows the finished figure. */
export function Played({
  children,
  line,
  linear = false,
  ms = 2400,
}: {
  children: (p: number) => ReactNode;
  // Where in the window the figure's top has to reach before it plays. A tall figure waits until
  // more of it is on screen.
  line?: number;
  // Plays at a steady pace, for a figure whose beats are spaced out along its progress.
  linear?: boolean;
  ms?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const seen = useInView(ref, line);
  // Anyone who asked for less motion gets the finished figure from the start, not once it's in view.
  // Once finished it stays finished, even if reduced motion is turned off afterward.
  const reduce = useReducedMotion();
  const [done, setDone] = useState(false);
  useEffect(() => {
    if (seen || reduce) setDone(true);
  }, [seen, reduce]);
  const p = useTween(done || seen || reduce ? 1 : 0, ms, false, linear);
  return <div ref={ref}>{children(p)}</div>;
}

/** A figure whose progress follows the scroll, without pinning: 0 while its middle is `from` of the
 *  way down the window, 1 once its middle reaches `to`, so scrolling back plays it in reverse. Under
 *  reduced motion it shows the finished figure. */
export function Scrubbed({
  children,
  from = 0.9,
  to = 0.3,
}: {
  children: (p: number) => ReactNode;
  from?: number;
  to?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const reduce = useReducedMotion();
  const [p, setP] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el || reduce) return;
    let frame = 0;
    let shown = -1;
    const target = () => {
      const r = el.getBoundingClientRect();
      const vh = window.innerHeight;
      const mid = r.top + r.height / 2;
      return Math.min(1, Math.max(0, (vh * from - mid) / (vh * (from - to))));
    };
    // The shown progress covers part of the distance each frame, so a jump in the scroll position
    // eases in rather than snapping.
    const tick = () => {
      frame = 0;
      const t = target();
      shown = shown < 0 ? t : shown + (t - shown) * 0.18;
      if (Math.abs(t - shown) < 0.0005) shown = t;
      setP(shown);
      if (shown !== t) frame = requestAnimationFrame(tick);
    };
    const wake = () => {
      if (!frame) frame = requestAnimationFrame(tick);
    };
    wake();
    window.addEventListener('scroll', wake, { passive: true });
    window.addEventListener('resize', wake);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('scroll', wake);
      window.removeEventListener('resize', wake);
    };
  }, [from, to, reduce]);
  return <div ref={ref}>{children(reduce ? 1 : p)}</div>;
}

// How long a picked side takes to arrive.
const PICK_MS = 320;

/** A figure that slowly plays forward and back while on screen, holding at each end. It runs off one
 *  clock: a marker that sweeps across and back, and the figure switches each time the marker passes
 *  the middle, so a toggle can show the marker and the reader sees each switch coming. `pick` goes to
 *  one end and holds there while the pointer stays on the figure; once it leaves, the loop carries
 *  on from that end. On a touch screen a pick holds until the next one. */
export function Looped({
  children,
  hold = 2800,
  move = 1800,
}: {
  // `wait` is where the marker is, from 0 at the left to 1 at the right; null while the reader's pick
  // holds the figure.
  children: (
    p: number,
    pick: (end: 0 | 1) => void,
    wait: number | null,
  ) => ReactNode;
  hold?: number;
  move?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const reduce = useReducedMotion();
  const [p, setP] = useState(0);
  const [wait, setWait] = useState<number | null>(0);
  // The figure's height can change between its two ends, so the slot keeps the tallest it has been,
  // and the page under it never moves.
  const [tallest, setTallest] = useState(0);
  // Under reduced motion there's no loop: the figure shows whichever end was picked last, kept in
  // state so that every pick renders, including the first pick of end 0.
  const [still, setStill] = useState<0 | 1>(1);
  // The end the reader picked, while it holds; and the end to carry on from once it lets go.
  const picked = useRef<0 | 1 | null>(null);
  const resume = useRef<0 | 1 | null>(null);
  const shown = useRef(0);

  useEffect(() => {
    const el = ref.current?.firstElementChild;
    if (!el) return;
    const height = () => el.getBoundingClientRect().height;
    // A figure hidden at this width measures 0 and holds no space.
    const watcher = new ResizeObserver(() =>
      setTallest((h) => (height() === 0 ? 0 : Math.max(h, height()))),
    );
    watcher.observe(el);
    // A new window width lays the figure out again, so the tallest starts over from what it is now.
    let width = innerWidth;
    const resized = () => {
      if (innerWidth === width) return;
      width = innerWidth;
      setTallest(height());
    };
    addEventListener('resize', resized);
    return () => {
      watcher.disconnect();
      removeEventListener('resize', resized);
    };
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el || reduce) return;
    // One sweep across takes a hold and a move; the figure's move is centered on the middle.
    const sweepMs = hold + move;
    const lap = 2 * sweepMs;
    const half = move / sweepMs / 2;
    let clock = 0;
    let marker = 0;
    let last = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      const target = picked.current;
      if (target !== null) {
        // Go to the picked end quickly: it's an answer to a click, not part of the loop.
        const step = (now - last) / PICK_MS;
        const d = target - shown.current;
        shown.current += Math.sign(d) * Math.min(Math.abs(d), step);
      } else {
        if (resume.current !== null) {
          // Pick up the loop at the end the reader left it on, so nothing jumps.
          clock = resume.current === 1 ? sweepMs : 0;
          resume.current = null;
        }
        clock = (clock + now - last) % lap;
        // Across and back, slowing into each end like a pendulum instead of stopping dead.
        const there = clock < sweepMs ? clock / sweepMs : 2 - clock / sweepMs;
        marker = 0.5 - 0.5 * Math.cos(Math.PI * there);
        shown.current = ease(
          Math.min(1, Math.max(0, (marker - (0.5 - half)) / (2 * half))),
        );
      }
      last = now;
      setP(shown.current);
      setWait(target !== null ? null : marker);
      frame = requestAnimationFrame(tick);
    };
    // The loop runs only while the figure is on screen. Off screen no frames run at all, and the
    // clock picks up where it stopped, without jumping, when it comes back.
    const io = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) {
        if (!frame) {
          last = performance.now();
          frame = requestAnimationFrame(tick);
        }
      } else {
        cancelAnimationFrame(frame);
        frame = 0;
        // Scrolled away: a pick left in place by a sideways exit lets go.
        if (picked.current !== null) {
          resume.current = picked.current;
          picked.current = null;
        }
      }
    });
    io.observe(el);
    return () => {
      cancelAnimationFrame(frame);
      io.disconnect();
    };
  }, [hold, move, reduce]);

  return (
    <div
      onPointerLeave={(e) => {
        if (
          e.pointerType === 'mouse' &&
          picked.current !== null &&
          leftVertically(e)
        ) {
          resume.current = picked.current;
          picked.current = null;
        }
      }}
      ref={ref}
      style={{ minHeight: tallest || undefined }}
    >
      {children(
        reduce ? still : p,
        (end) => {
          if (reduce) setStill(end);
          else picked.current = end;
        },
        reduce ? null : wait,
      )}
    </div>
  );
}
