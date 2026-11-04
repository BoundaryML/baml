'use client';

import { type RefObject, useCallback, useEffect, useState } from 'react';

// Scroll-driven figures. A figure sits in a track: a pinned block followed by a spacer. While the
// track passes through the window, the pinned block sticks in the middle of the window and the hook
// reports how far through the track the reader is, from 0 to 1. Where the block isn't sticky (a
// narrow screen, say), progress follows the block, or the figure inside it, as it crosses the window.

// The nav is 72px tall. A pinned block keeps clear of it.
const NAV_CLEARANCE = 88;
// How much of the remaining distance the shown progress covers each frame, so a jump in the scroll
// position eases in rather than snapping.
const EASE = 0.18;

const clamp = (v: number) => Math.min(1, Math.max(0, v));

/** Where the reader is in the track, from 0 to 1. Also centers the pinned block under the nav. */
function measure(
  track: HTMLElement,
  pin: HTMLElement,
  follow: HTMLElement,
): number {
  const vh = window.innerHeight;
  const h = pin.offsetHeight;
  // Center the pinned block in the space under the nav.
  pin.style.top = `${Math.max(NAV_CLEARANCE, (vh - h) / 2 + NAV_CLEARANCE / 2)}px`;
  if (getComputedStyle(pin).position === 'sticky') {
    const r = track.getBoundingClientRect();
    const range = r.height - h;
    if (range <= 0) return 1;
    return clamp((Number.parseFloat(pin.style.top) - r.top) / range);
  }
  // Not pinned: from the figure's top at 90% of the window to its top at 25%.
  const top = follow.getBoundingClientRect().top;
  return clamp((vh * 0.9 - top) / (vh * 0.65));
}

/** Progress through a track, eased; a way to scroll the window to a given progress; and whether the
 *  reader prefers reduced motion, in which case progress stays at 1, the finished figure. Progress is
 *  null until the page has been measured. `follow` is what progress tracks when the block isn't
 *  pinned; it defaults to the block. */
export function useScrollProgress(
  track: RefObject<HTMLElement | null>,
  pin: RefObject<HTMLElement | null>,
  follow?: RefObject<HTMLElement | null>,
) {
  const [progress, setProgress] = useState<number | null>(null);
  const [still, setStill] = useState(false);

  useEffect(() => {
    const t = track.current;
    const p = pin.current;
    const f = follow?.current ?? p;
    if (!t || !p || !f) return;
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)');
    let frame = 0;
    let shown = measure(t, p, f);
    let visible = false;

    const tick = () => {
      frame = 0;
      setStill(reduce.matches);
      if (reduce.matches) {
        setProgress(1);
        return;
      }
      const target = measure(t, p, f);
      shown += (target - shown) * EASE;
      if (Math.abs(target - shown) < 0.0005) shown = target;
      setProgress(shown);
      if (visible && shown !== target) frame = requestAnimationFrame(tick);
    };
    const wake = () => {
      if (!frame) frame = requestAnimationFrame(tick);
    };

    setStill(reduce.matches);
    setProgress(reduce.matches ? 1 : shown);
    // Only work while the track is near the window.
    const observer = new IntersectionObserver(
      ([entry]) => {
        visible = entry.isIntersecting;
        if (visible) wake();
      },
      { rootMargin: '50% 0px' },
    );
    observer.observe(t);
    window.addEventListener('scroll', wake, { passive: true });
    window.addEventListener('resize', wake);
    reduce.addEventListener('change', wake);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener('scroll', wake);
      window.removeEventListener('resize', wake);
      reduce.removeEventListener('change', wake);
    };
  }, [track, pin, follow]);

  /** Scrolls the window so the track is at progress `to`. */
  const scrollTo = useCallback(
    (to: number) => {
      const t = track.current;
      const p = pin.current;
      if (!t || !p) return;
      const r = t.getBoundingClientRect();
      const top = Number.parseFloat(p.style.top) || NAV_CLEARANCE;
      const y = window.scrollY + r.top - top + to * (r.height - p.offsetHeight);
      window.scrollTo({ behavior: 'smooth', top: y });
    },
    [track, pin],
  );

  return [progress, scrollTo, still] as const;
}

/** A 0 to 1 ramp across [from, to], eased in and out. */
export function ramp(p: number, from: number, to: number) {
  const x = clamp((p - from) / (to - from));
  return x * x * (3 - 2 * x);
}
