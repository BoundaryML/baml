'use client';

import { type ReactNode, useEffect, useRef } from 'react';

// How far the painting moves for each pixel scrolled. The letters move at full speed, so the painting
// falling behind them reads as depth, as if looking through the letters at something further away.
const PARALLAX = 0.55;

/** The hero headline, cut out of the painting, which shifts behind the letters as the page scrolls. */
export default function PaintedHeadline({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const el = ref.current;
    if (!el) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      // Stops before the painting's edge can show inside the letters.
      const limit = el.offsetHeight * 1.2;
      const shift = Math.min(window.scrollY * PARALLAX, limit);
      el.style.setProperty('--v2-scroll', `${shift.toFixed(1)}px`);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, []);

  return (
    <h1 className="v2-h1 v2-painted pb-[0.08em]" ref={ref}>
      {children}
    </h1>
  );
}
