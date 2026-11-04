'use client';

import { useEffect, useRef } from 'react';

export type Frame = {
  ctx: CanvasRenderingContext2D;
  /** Milliseconds since the last frame, capped so a background tab doesn't jump the scene forward. */
  dt: number;
  height: number;
  width: number;
};

/**
 * Calls draw on every animation frame, on a canvas kept at its box's size and the screen's pixel density.
 * With trail set, each frame fades the last one by that much instead of clearing it, so moving things leave
 * streaks. With reduced motion, the scene runs a few seconds ahead once and stays still.
 */
export function useCanvas(draw: (frame: Frame) => void, trail = 0) {
  const ref = useRef<HTMLCanvasElement>(null);
  const latest = useRef(draw);
  latest.current = draw;

  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext('2d');
    if (!canvas || !ctx) return;
    const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    let width = 0;
    let height = 0;

    const paint = (dt: number) => {
      if (trail && !still) {
        ctx.globalCompositeOperation = 'destination-out';
        ctx.fillStyle = `rgb(0 0 0 / ${trail})`;
        ctx.fillRect(0, 0, width, height);
        ctx.globalCompositeOperation = 'source-over';
      } else {
        ctx.clearRect(0, 0, width, height);
      }
      latest.current({ ctx, dt, height, width });
    };

    const fit = () => {
      const box = canvas.getBoundingClientRect();
      const dpr = Math.min(window.devicePixelRatio, 2);
      width = box.width;
      height = box.height;
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      if (still) for (let i = 0; i < 240; i++) paint(16);
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(canvas);
    if (still) return () => observer.disconnect();

    let frame = 0;
    let last = performance.now();
    const tick = (now: number) => {
      // A frame's timestamp can be from just before the effect ran, so the first gap can be negative.
      paint(Math.max(0, Math.min(now - last, 50)));
      last = now;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [trail]);

  return ref;
}

export const clamp = (n: number) => Math.min(1, Math.max(0, n));
export const easeOut = (t: number) => 1 - (1 - t) ** 3;
export const easeInOut = (t: number) =>
  t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;
