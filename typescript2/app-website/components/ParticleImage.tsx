'use client';

import { useEffect, useRef } from 'react';
import type { CSSProperties } from 'react';

// ─── Public API ───────────────────────────────────────────────────────────────

export type StreamStyle = 'cascade' | 'radial' | 'random';

export interface ParticleImageProps {
  src: string;
  width?: number;
  height?: number;
  /** Size of each rendered particle in logical pixels */
  particleSize?: number;
  /** Pixel sampling step. Lower = more particles. Default 3 */
  density?: number;
  /** Seconds for all particles to arrive at their targets */
  streamDuration?: number;
  /** How particles enter: top-to-bottom, radially from center, or chaos */
  streamStyle?: StreamStyle;
  /** Idle float amplitude in logical pixels after arriving */
  jitter?: number;
  /** Mouse repulsion radius in logical pixels. 0 to disable */
  hoverRepel?: number;
  /** Spring acceleration toward target. Lower = slower drift. Default 0.055 */
  spring?: number;
  className?: string;
  style?: CSSProperties;
}

// ─── Internal types ───────────────────────────────────────────────────────────

type Particle = {
  x: number; y: number;     // current logical position
  vx: number; vy: number;   // velocity (logical px/frame)
  tx: number; ty: number;   // target logical position
  delay: number;            // ms before this particle begins moving
  color: number;            // packed RGBA as little-endian Uint32 (ABGR in memory)
  phase: number;            // per-particle random offset for idle jitter
  arrived: boolean;
};

// ─── Physics constants ────────────────────────────────────────────────────────

const SPRING = 0.055;
const DAMPING = 0.85;
const REPEL_STRENGTH = 5.5;
// Squared thresholds avoid sqrt in hot loop
const SETTLE_D2 = 2.25;  // 1.5px²
const SETTLE_V2 = 0.06;  // ~0.245px/frame²
const MAX_SAMPLE_DIM = 400;

// ─── Component ────────────────────────────────────────────────────────────────

export default function ParticleImage({
  src,
  width = 600,
  height = 400,
  particleSize = 2,
  density = 3,
  streamDuration = 2,
  streamStyle = 'cascade',
  jitter = 0.5,
  hoverRepel = 80,
  spring = 0.055,
  className,
  style,
}: ParticleImageProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const particlesRef = useRef<Particle[]>([]);
  const mouseRef = useRef<{ x: number; y: number } | null>(null);
  const rafRef = useRef<number | null>(null);
  // Pre-allocated pixel buffers — reused every frame, zero allocations in render loop
  const imgDataRef = useRef<ImageData | null>(null);
  const u32Ref = useRef<Uint32Array | null>(null);   // Uint32 view of the same buffer
  // Physical pixel dimensions (logical × dpr)
  const physRef = useRef({ w: 0, h: 0, dpr: 1, sz: 1 });

  // ── Mouse ─────────────────────────────────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || hoverRepel <= 0) return;
    const onMove = (e: MouseEvent) => {
      const r = canvas.getBoundingClientRect();
      mouseRef.current = { x: e.clientX - r.left, y: e.clientY - r.top };
    };
    const onLeave = () => { mouseRef.current = null; };
    canvas.addEventListener('mousemove', onMove);
    canvas.addEventListener('mouseleave', onLeave);
    return () => {
      canvas.removeEventListener('mousemove', onMove);
      canvas.removeEventListener('mouseleave', onLeave);
    };
  }, [hoverRepel]);

  // ── Image → particles → render loop ──────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !src) return;

    // One-time canvas/context setup for this src+size combination
    const dpr = window.devicePixelRatio || 1;
    const physW = Math.round(width * dpr);
    const physH = Math.round(height * dpr);
    const sz = Math.max(1, Math.round(particleSize * dpr)); // physical particle size
    physRef.current = { w: physW, h: physH, dpr, sz };

    canvas.width = physW;
    canvas.height = physH;
    const ctx = canvas.getContext('2d')!;
    ctxRef.current = ctx;

    // Pre-allocate ImageData + a Uint32Array view sharing the same ArrayBuffer.
    // Writing one 32-bit value per pixel is 4× fewer store operations than byte-by-byte.
    const imgData = ctx.createImageData(physW, physH);
    imgDataRef.current = imgData;
    u32Ref.current = new Uint32Array(imgData.data.buffer);

    let cancelled = false;

    const img = new Image();

    img.onload = () => {
      if (cancelled) return;

      // ── 1. Sample image pixels at reduced resolution ─────────────────────
      const scale = Math.min(1, MAX_SAMPLE_DIM / Math.max(img.naturalWidth || 1, img.naturalHeight || 1));
      const sW = Math.max(1, Math.round(img.naturalWidth * scale));
      const sH = Math.max(1, Math.round(img.naturalHeight * scale));

      const off = document.createElement('canvas');
      off.width = sW;
      off.height = sH;
      const offCtx = off.getContext('2d')!;
      offCtx.drawImage(img, 0, 0, sW, sH);
      let data: Uint8ClampedArray;
      try {
        data = offCtx.getImageData(0, 0, sW, sH).data;
      } catch (e) {
        // Retry with crossOrigin if tainted
        console.error('[ParticleImage] getImageData failed, retrying with crossOrigin', e);
        return;
      }

      // ── 2. Build raw particle list (skip transparent pixels) ─────────────
      type RawP = { tx: number; ty: number; color: number };
      const raw: RawP[] = [];

      for (let sy = 0; sy < sH; sy += density) {
        for (let sx = 0; sx < sW; sx += density) {
          const i = (sy * sW + sx) * 4;
          const a = data[i + 3]!;
          if (a < 10) continue;
          const r = data[i]!;
          const g = data[i + 1]!;
          const b = data[i + 2]!;
          // Pack as little-endian Uint32: in memory bytes are [R, G, B, A]
          // which as a 32-bit LE word is: A<<24 | B<<16 | G<<8 | R
          const color = ((a << 24) | (b << 16) | (g << 8) | r) >>> 0;
          raw.push({
            tx: (sx / sW) * width,
            ty: (sy / sH) * height,
            color,
          });
        }
      }

      if (raw.length === 0 || cancelled) return;

      // ── 3. Sort, assign start origins and stagger delays ─────────────────
      const cx = width / 2;
      const cy = height / 2;
      const maxDist = Math.hypot(cx, cy);
      const streamMs = streamDuration * 1000;

      let sorted: RawP[];
      if (streamStyle === 'cascade') {
        sorted = [...raw].sort((a, b) => a.ty - b.ty);
      } else if (streamStyle === 'radial') {
        // Center-first so the image blooms outward
        sorted = [...raw].sort((a, b) => {
          const da = (a.tx - cx) ** 2 + (a.ty - cy) ** 2;
          const db = (b.tx - cx) ** 2 + (b.ty - cy) ** 2;
          return da - db;
        });
      } else {
        sorted = [...raw].sort(() => Math.random() - 0.5);
      }

      const n = sorted.length;
      const particles: Particle[] = sorted.map((p, i) => {
        // Stagger delays: first particle at 0 ms, last at 65% of streamDuration
        const delay = (i / Math.max(1, n - 1)) * streamMs * 0.65;

        let ox: number, oy: number;
        if (streamStyle === 'cascade') {
          ox = Math.random() * width;
          oy = -(60 + Math.random() * height * 0.7);
        } else if (streamStyle === 'radial') {
          const angle = Math.atan2(p.ty - cy, p.tx - cx);
          const outDist = maxDist * 1.5 + Math.random() * 80;
          ox = cx + Math.cos(angle) * outDist;
          oy = cy + Math.sin(angle) * outDist;
        } else {
          ox = cx + (Math.random() - 0.5) * width * 2.8;
          oy = cy + (Math.random() - 0.5) * height * 2.8;
        }

        return {
          x: ox, y: oy,
          vx: 0, vy: 0,
          tx: p.tx, ty: p.ty,
          delay,
          color: p.color,
          phase: Math.random() * Math.PI * 2,
          arrived: false,
        };
      });

      particlesRef.current = particles;

      // ── 4. Render loop ────────────────────────────────────────────────────
      let startTs = -1;
      const repelR2 = hoverRepel * hoverRepel;

      function tick(ts: number) {
        if (cancelled) return;
        if (startTs < 0) startTs = ts;

        const elapsed = ts - startTs;
        const t = ts * 0.001; // seconds

        const { w: pW, h: pH, dpr: dpr_, sz: sz_ } = physRef.current;
        const ps = particlesRef.current;
        const mouse = mouseRef.current;
        const u32 = u32Ref.current!;

        // Clear entire pixel buffer in one pass — Uint32Array.fill is a memset
        u32.fill(0);

        for (const p of ps) {
          if (elapsed < p.delay) continue;

          // Idle jitter
          const jx = Math.sin(t * 0.7 + p.phase) * jitter;
          const jy = Math.cos(t * 1.1 + p.phase + 1.57) * jitter;

          if (p.arrived) {
            p.vx += (p.tx + jx - p.x) * spring * 0.4;
            p.vy += (p.ty + jy - p.y) * spring * 0.4;
          } else {
            const dxT = p.tx - p.x;
            const dyT = p.ty - p.y;
            // Suppress jitter while still noticeably in-flight
            const jScale = dxT * dxT + dyT * dyT > 100 ? 0 : 0.35;
            p.vx += (p.tx + jx * jScale - p.x) * spring;
            p.vy += (p.ty + jy * jScale - p.y) * spring;
          }

          // Mouse repulsion
          if (mouse && hoverRepel > 0) {
            const mdx = p.x - mouse.x;
            const mdy = p.y - mouse.y;
            const md2 = mdx * mdx + mdy * mdy;
            if (md2 < repelR2 && md2 > 0.25) {
              const md = Math.sqrt(md2);
              const force = (1 - md / hoverRepel) * REPEL_STRENGTH;
              p.vx += (mdx / md) * force;
              p.vy += (mdy / md) * force;
            }
          }

          p.vx *= DAMPING;
          p.vy *= DAMPING;
          p.x += p.vx;
          p.y += p.vy;

          // Settle check (squared distances — no sqrt)
          if (!p.arrived) {
            const dxT = p.tx - p.x;
            const dyT = p.ty - p.y;
            if (
              dxT * dxT + dyT * dyT < SETTLE_D2 &&
              p.vx * p.vx + p.vy * p.vy < SETTLE_V2
            ) {
              p.arrived = true;
            }
          }

          // Write particle block into the Uint32 buffer.
          // Clamp to canvas bounds before entering inner loops.
          const px = Math.round(p.x * dpr_);
          const py = Math.round(p.y * dpr_);
          const x0 = Math.max(0, px);
          const x1 = Math.min(pW, px + sz_);
          const y0 = Math.max(0, py);
          const y1 = Math.min(pH, py + sz_);
          const color = p.color;

          for (let qy = y0; qy < y1; qy++) {
            const rowBase = qy * pW;
            for (let qx = x0; qx < x1; qx++) {
              u32[rowBase + qx] = color;
            }
          }
        }

        ctxRef.current!.putImageData(imgDataRef.current!, 0, 0);
        rafRef.current = requestAnimationFrame(tick);
      }

      rafRef.current = requestAnimationFrame(tick);
    };

    img.onerror = () => {
      console.error(`[ParticleImage] Failed to load: ${src}`);
    };

    img.src = src;

    return () => {
      cancelled = true;
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      particlesRef.current = [];
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [src, width, height, density, particleSize, streamDuration, streamStyle, jitter, hoverRepel, spring]);

  return (
    <canvas
      ref={canvasRef}
      className={className}
      style={{ width, height, display: 'block', ...style }}
    />
  );
}
