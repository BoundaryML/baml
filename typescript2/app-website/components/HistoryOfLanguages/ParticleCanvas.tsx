'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import ParticleImage from '@/components/ParticleImage';
import { BAML_SNIPPET } from './constants';
import type { ShapeId } from './shapes';
import { buildShape } from './shapes';
import styles from './styles.module.css';
import { useParticleEngine } from './useParticleEngine';
import type { EnginePhase } from './useParticleEngine';

// Logical canvas size. Physics run in these coordinates.
const CANVAS_W = 600;
const CANVAS_H = 300;

// Shapes that use image-based particle sampling instead of hand-crafted paths.
// Drop the corresponding PNG/SVG in /public/ and map the shapeId here.
const IMAGE_SHAPES: Partial<Record<ShapeId, string>> = {
  sphinx: '/sphinx.svg',
};

type ParticleCanvasProps = {
  shapeId: ShapeId;
  active: boolean;
  isBaml?: boolean;
};

function tokenClass(token: string): string {
  if (/^(function|client|prompt|enum)$/.test(token)) return styles.tokenKeyword;
  if (/^(Category|ClassifyMessage|Greeting|Question|Complaint|Other)$/.test(token))
    return styles.tokenType;
  if (/^".*"$/.test(token) || /^#"$/.test(token) || /^"#$/.test(token))
    return styles.tokenString;
  return styles.tokenPlain;
}

export function ParticleCanvas({ active, isBaml = false, shapeId }: ParticleCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const mouseRef = useRef<{ x: number; y: number } | null>(null);
  const [phase, setPhase] = useState<EnginePhase>('idle');
  const [targets, setTargets] = useState<{ x: number; y: number }[]>([]);

  // If this shape has an image source, delegate entirely to ParticleImage
  const imageSrc = IMAGE_SHAPES[shapeId];

  // Build shape target points after mount (client-side, Math.random safe)
  useEffect(() => {
    if (imageSrc) return; // image-based shapes don't use this engine
    setTargets(buildShape(shapeId, CANVAS_W, CANVAS_H, 280));
  }, [shapeId, imageSrc]);

  // Scale canvas for device pixel ratio
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = CANVAS_W * dpr;
    canvas.height = CANVAS_H * dpr;
    const ctx = canvas.getContext('2d');
    if (ctx) ctx.scale(dpr, dpr);
  }, []);

  const handlePhaseChange = useCallback((p: EnginePhase) => {
    setPhase(p);
  }, []);

  useParticleEngine({
    canvasRef,
    targets,
    active,
    isBaml,
    mouseRef,
    onPhaseChange: handlePhaseChange,
  });

  const onMouseMove = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    // Map from display coords to logical canvas coords
    const scaleX = CANVAS_W / rect.width;
    const scaleY = CANVAS_H / rect.height;
    mouseRef.current = {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  }, []);

  const onMouseLeave = useCallback(() => {
    mouseRef.current = null;
  }, []);

  const codeVisible = phase === 'done';

  const reducedMotion = useMemo(() => {
    if (typeof window === 'undefined') return false;
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  }, []);

  // ── Image-based particle panels (e.g. sphinx) ─────────────────────────────
  if (imageSrc && !isBaml) {
    return (
      <div className={styles.canvasWrap}>
        {active && (
          <ParticleImage
            src={imageSrc}
            width={CANVAS_W}
            height={CANVAS_H}
            particleSize={2}
            density={3}
            streamDuration={2.5}
            streamStyle="cascade"
            jitter={0.6}
            hoverRepel={80}
            style={{ width: '100%', maxWidth: CANVAS_W, height: 'auto' }}
          />
        )}
      </div>
    );
  }

  return (
    <div
      className={styles.canvasWrap}
      onMouseMove={reducedMotion ? undefined : onMouseMove}
      onMouseLeave={reducedMotion ? undefined : onMouseLeave}
    >
      <canvas
        ref={canvasRef}
        style={{
          width: '100%',
          maxWidth: `${CANVAS_W}px`,
          height: 'auto',
          display: 'block',
        } as CSSProperties}
        aria-hidden
      />

      {isBaml && (
        <div className={codeVisible ? styles.bamlOverlayVisible : styles.bamlOverlay}>
          <pre className={styles.morphCode}>
            {BAML_SNIPPET.split('\n').map((line, lineIdx) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: stable static content
              <div key={lineIdx} className={styles.codeLine}>
                {line.split(/(\s+|[{}()":,#>-])/g).map((token, tokenIdx) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: stable static content
                  <span key={tokenIdx} className={tokenClass(token)}>
                    {token}
                  </span>
                ))}
              </div>
            ))}
          </pre>
        </div>
      )}
    </div>
  );
}
