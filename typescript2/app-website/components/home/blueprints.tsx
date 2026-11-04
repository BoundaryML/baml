'use client';

import { useEffect, useState } from 'react';

// Section 2's drawing. Two blueprints share the sheet and cross-fade on their own, starting with the
// BAML stack. Under reduced motion the sheet holds on the stack.

const DRAWINGS = [
  {
    alt: 'The layers of the BAML stack drawn as a blueprint, from the virtual machine at the bottom to dashboards, alerts, and feature flags at the top.',
    src: '/home/stack.svg',
  },
  {
    alt: 'Raptor 1, Raptor 2, and Raptor 3 engines side by side, drawn as a blueprint. Each one has fewer external parts than the last.',
    src: '/home/raptors.svg',
  },
];

const HOLD_MS = 6000;

export default function Blueprints() {
  const [shown, setShown] = useState(0);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const timer = window.setInterval(
      () => setShown((i) => (i + 1) % DRAWINGS.length),
      HOLD_MS,
    );
    return () => window.clearInterval(timer);
  }, []);

  return (
    <figure className="v4-blueprint-sheet grid rounded-2xl overflow-hidden border border-rule">
      {DRAWINGS.map((d, i) => (
        // biome-ignore lint/performance/noImgElement: a static SVG drawing, shown at its own size
        <img
          alt={d.alt}
          aria-hidden={i !== shown}
          className={`col-start-1 row-start-1 block w-full h-auto transition-opacity duration-1000 ${i === shown ? 'opacity-100' : 'opacity-0'}`}
          height={720}
          key={d.src}
          src={d.src}
          width={960}
        />
      ))}
    </figure>
  );
}
