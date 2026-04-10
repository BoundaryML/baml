'use client';

import createGlobe, { type COBEOptions } from 'cobe';
import { useMotionValue, useSpring } from 'motion/react';
import { useTheme } from 'next-themes';
import { useEffect, useMemo, useRef } from 'react';
import { cn } from '@/lib/utils';

const MOVEMENT_DAMPING = 1400;

const GLOBE_CONFIG: COBEOptions = {
  baseColor: [1, 1, 1],
  dark: 0,
  devicePixelRatio: 2,
  diffuse: 0.4,
  glowColor: [1, 1, 1],
  height: 800,
  mapBrightness: 1.2,
  mapSamples: 16000,
  markerColor: [251 / 255, 100 / 255, 21 / 255],
  markers: [
    { location: [14.5995, 120.9842], size: 0.12 },
    { location: [19.076, 72.8777], size: 0.4 },
    { location: [23.8103, 90.4125], size: 0.2 },
    { location: [30.0444, 31.2357], size: 0.28 },
    { location: [39.9042, 116.4074], size: 0.32 },
    { location: [-23.5505, -46.6333], size: 0.4 },
    { location: [19.4326, -99.1332], size: 0.4 },
    { location: [40.7128, -74.006], size: 0.4 },
    { location: [34.6937, 135.5022], size: 0.2 },
    { location: [41.0082, 28.9784], size: 0.24 },
  ],
  onRender: () => {},
  phi: 0,
  theta: 0.3,
  width: 800,
};

// Define color configurations for light and dark modes
const COLORS = {
  dark: {
    base: [0.4, 0.4, 0.4] as [number, number, number],
    glow: [0.24, 0.24, 0.27] as [number, number, number],
    marker: [87 / 255, 179 / 255, 148 / 255] as [number, number, number],
  },
  light: {
    base: [1, 1, 1] as [number, number, number],
    glow: [1, 1, 1] as [number, number, number],
    marker: [87 / 255, 179 / 255, 148 / 255] as [number, number, number],
  },
};

export function Globe({
  className,
  config = GLOBE_CONFIG,
}: {
  className?: string;
  config?: COBEOptions;
}) {
  const { theme } = useTheme();
  const isDarkMode = theme === 'dark';

  const phiRef = useRef(0);
  const widthRef = useRef(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const pointerInteracting = useRef<number | null>(null);
  const pointerInteractionMovement = useRef(0);

  const r = useMotionValue(0);
  const rs = useSpring(r, {
    damping: 30,
    mass: 1,
    stiffness: 100,
  });

  const finalConfig = useMemo(
    () => ({
      ...config,
      baseColor: isDarkMode ? COLORS.dark.base : COLORS.light.base,
      dark: isDarkMode ? 1 : 0,
      diffuse: isDarkMode ? 0.5 : 0.4,
      glowColor: isDarkMode ? COLORS.dark.glow : COLORS.light.glow,
      mapBrightness: isDarkMode ? 1.4 : 1.2,
      markerColor: isDarkMode ? COLORS.dark.marker : COLORS.light.marker,
    }),
    [config, isDarkMode],
  );

  const updatePointerInteraction = (value: number | null) => {
    pointerInteracting.current = value;
    if (canvasRef.current) {
      canvasRef.current.style.cursor = value !== null ? 'grabbing' : 'grab';
    }
  };

  const updateMovement = (clientX: number) => {
    if (pointerInteracting.current !== null) {
      const delta = clientX - pointerInteracting.current;
      pointerInteractionMovement.current = delta;
      r.set(r.get() + delta / MOVEMENT_DAMPING);
    }
  };

  useEffect(() => {
    const onResize = () => {
      if (canvasRef.current) {
        widthRef.current = canvasRef.current.offsetWidth;
      }
    };

    window.addEventListener('resize', onResize);
    onResize();

    let globe: ReturnType<typeof createGlobe> | null = null;

    if (canvasRef.current) {
      globe = createGlobe(canvasRef.current, {
        ...finalConfig,
        height: widthRef.current * 2,
        onRender: (state) => {
          if (!pointerInteracting.current) phiRef.current += 0.005;
          state.phi = phiRef.current + rs.get();
          state.width = widthRef.current * 2;
          state.height = widthRef.current * 2;
        },
        width: widthRef.current * 2,
      });

      setTimeout(() => {
        if (canvasRef.current) {
          canvasRef.current.style.opacity = '1';
        }
      }, 0);
    }
    return () => {
      if (globe) {
        globe.destroy();
      }
      window.removeEventListener('resize', onResize);
    };
  }, [rs, finalConfig]);

  return (
    <div
      className={cn(
        'absolute inset-0 mx-auto aspect-[1/1] w-full max-w-[600px]',
        className,
      )}
    >
      <canvas
        className={cn(
          'size-full opacity-0 transition-opacity duration-500 [contain:layout_paint_size]',
        )}
        onMouseMove={(e) => updateMovement(e.clientX)}
        onPointerDown={(e) => {
          pointerInteracting.current = e.clientX;
          updatePointerInteraction(e.clientX);
        }}
        onPointerOut={() => updatePointerInteraction(null)}
        onPointerUp={() => updatePointerInteraction(null)}
        onTouchMove={(e) =>
          e.touches[0] && updateMovement(e.touches[0].clientX)
        }
        ref={canvasRef}
      />
    </div>
  );
}
