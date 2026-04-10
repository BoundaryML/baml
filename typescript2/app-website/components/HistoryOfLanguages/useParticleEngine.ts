'use client';

import { useCallback, useEffect, useRef } from 'react';
import type { ShapePoint } from './shapes';

export type Particle = {
  x: number;
  y: number;
  vx: number;
  vy: number;
  tx: number; // target x
  ty: number; // target y
  ox: number; // origin (spawn) x
  oy: number; // origin (spawn) y
  radius: number;
  alpha: number;
};

export type EnginePhase = 'idle' | 'assembling' | 'assembled' | 'dissolving' | 'done';

const SPRING = 0.045;
const DAMPING = 0.87;
const REPEL_RADIUS = 85;
const REPEL_FORCE = 3.5;

// Assembled threshold: particle is "settled" when very close to target with low velocity
const SETTLED_POS = 1.8;
const SETTLED_VEL = 0.25;

type UseParticleEngineArgs = {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  targets: ShapePoint[]; // pixel-space target positions
  active: boolean;
  isBaml: boolean;
  mouseRef: React.RefObject<{ x: number; y: number } | null>;
  onPhaseChange: (phase: EnginePhase) => void;
};

export function useParticleEngine({
  active,
  canvasRef,
  isBaml,
  mouseRef,
  onPhaseChange,
  targets,
}: UseParticleEngineArgs) {
  const particlesRef = useRef<Particle[]>([]);
  const phaseRef = useRef<EnginePhase>('idle');
  const rafRef = useRef<number | null>(null);
  const dissolveTriggerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Spawn particles scattered randomly within canvas
  const spawnParticles = useCallback((w: number, h: number) => {
    particlesRef.current = targets.map((t) => {
      const ox = Math.random() * w;
      const oy = Math.random() * h;
      return {
        x: ox,
        y: oy,
        vx: (Math.random() - 0.5) * 2.5,
        vy: (Math.random() - 0.5) * 2.5,
        tx: t.x,
        ty: t.y,
        ox,
        oy,
        radius: 1.2 + Math.random() * 0.8,
        alpha: 0.0, // fade in during assembly
      };
    });
  }, [targets]);

  const setPhase = useCallback(
    (p: EnginePhase) => {
      phaseRef.current = p;
      onPhaseChange(p);
    },
    [onPhaseChange],
  );

  // Main animation tick
  const tick = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const W = canvas.width / dpr;
    const H = canvas.height / dpr;
    const particles = particlesRef.current;
    const phase = phaseRef.current;
    const mouse = mouseRef.current;

    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = '#1a1a1a';

    let settledCount = 0;

    for (const p of particles) {
      // --- Physics ---
      if (phase === 'assembling' || phase === 'assembled') {
        // Spring force toward target
        p.vx += (p.tx - p.x) * SPRING;
        p.vy += (p.ty - p.y) * SPRING;
        // Fade in
        if (p.alpha < 1) p.alpha = Math.min(1, p.alpha + 0.025);
      } else if (phase === 'idle') {
        // Gentle drift when idle
        p.vx += (Math.random() - 0.5) * 0.25;
        p.vy += (Math.random() - 0.5) * 0.25;
        if (p.alpha < 0.35) p.alpha += 0.01;
      } else if (phase === 'dissolving') {
        // Drift outward from target, alpha drains
        p.vx += (p.x - p.tx) * 0.012;
        p.vy += (p.y - p.ty) * 0.012;
        p.alpha = Math.max(0, p.alpha - 0.013);
      }

      // Mouse repulsion (active during assembling and assembled)
      if (mouse && (phase === 'assembling' || phase === 'assembled')) {
        const mdx = p.x - mouse.x;
        const mdy = p.y - mouse.y;
        const mdist = Math.sqrt(mdx * mdx + mdy * mdy);
        if (mdist < REPEL_RADIUS && mdist > 0.5) {
          const force = (1 - mdist / REPEL_RADIUS) * REPEL_FORCE;
          p.vx += (mdx / mdist) * force;
          p.vy += (mdy / mdist) * force;
        }
      }

      // Damping
      p.vx *= DAMPING;
      p.vy *= DAMPING;
      p.x += p.vx;
      p.y += p.vy;

      // Wrap particles that drift off-screen during idle/dissolve
      if (phase === 'idle') {
        if (p.x < -10) p.x = W + 10;
        if (p.x > W + 10) p.x = -10;
        if (p.y < -10) p.y = H + 10;
        if (p.y > H + 10) p.y = -10;
      }

      // --- Settlement detection ---
      if (phase === 'assembling') {
        const dx = Math.abs(p.tx - p.x);
        const dy = Math.abs(p.ty - p.y);
        if (dx < SETTLED_POS && dy < SETTLED_POS && Math.abs(p.vx) < SETTLED_VEL && Math.abs(p.vy) < SETTLED_VEL) {
          settledCount++;
        }
      }

      // --- Draw ---
      if (p.alpha <= 0.01) continue;
      ctx.globalAlpha = p.alpha;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.radius, 0, Math.PI * 2);
      ctx.fill();
    }

    ctx.globalAlpha = 1;

    // Transition from assembling → assembled when 92% of particles settle
    if (phase === 'assembling' && settledCount >= particles.length * 0.92) {
      setPhase('assembled');
      if (isBaml) {
        dissolveTriggerRef.current = setTimeout(() => setPhase('dissolving'), 2000);
      }
    }

    // Transition from dissolving → done when all particles faded out
    if (phase === 'dissolving' && particles.every((p) => p.alpha <= 0.01)) {
      setPhase('done');
      rafRef.current = null;
      return; // stop loop
    }

    rafRef.current = requestAnimationFrame(tick);
  }, [canvasRef, isBaml, mouseRef, setPhase]);

  // Start / stop loop based on `active`
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    if (!active) {
      // Pause loop when panel leaves view
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      return;
    }

    // Panel became active: spawn particles and start assembling
    if (phaseRef.current === 'idle' && targets.length > 0) {
      const dpr = window.devicePixelRatio || 1;
      const W = canvas.width / dpr;
      const H = canvas.height / dpr;
      spawnParticles(W, H);
      setPhase('assembling');
    }

    // Start (or restart) the loop
    if (!rafRef.current) {
      rafRef.current = requestAnimationFrame(tick);
    }

    return () => {
      if (rafRef.current) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
    };
  }, [active, canvasRef, setPhase, spawnParticles, targets, tick]);

  // Cleanup dissolve timer on unmount
  useEffect(() => {
    return () => {
      if (dissolveTriggerRef.current) clearTimeout(dissolveTriggerRef.current);
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
    };
  }, []);
}
