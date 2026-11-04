'use client';

import { useState } from 'react';

// Section 2's drawing. Two blueprints share the sheet, and the buttons under it swap between them
// while we compare.

const DRAWINGS = [
  {
    alt: 'Raptor 1, Raptor 2, and Raptor 3 engines side by side, drawn as a blueprint. Each one has fewer external parts than the last.',
    name: 'Raptor engines',
    src: '/v4/raptors.svg',
  },
  {
    alt: 'The layers of the BAML stack drawn as a blueprint, from the virtual machine at the bottom to dashboards, alerts, and feature flags at the top.',
    name: 'The BAML stack',
    src: '/v4/stack.svg',
  },
];

export default function Blueprints() {
  const [shown, setShown] = useState(0);
  const drawing = DRAWINGS[shown];
  return (
    <figure>
      <div className="v4-blueprint-sheet rounded-2xl overflow-hidden border border-white/10">
        {/* biome-ignore lint/performance/noImgElement: a static SVG drawing, shown at its own size */}
        <img
          alt={drawing.alt}
          className="block w-full h-auto"
          height={720}
          key={drawing.src}
          src={drawing.src}
          width={960}
        />
      </div>
      <figcaption className="mt-3 flex justify-end gap-1.5">
        {DRAWINGS.map((d, i) => (
          <button
            aria-pressed={i === shown}
            className={`tap rounded-full border px-3 py-1 text-[13px] transition-colors ${i === shown ? 'border-white/25 bg-white/[0.08] text-white' : 'border-white/10 text-white/55 hover:text-white/80'}`}
            key={d.src}
            onClick={() => setShown(i)}
            type="button"
          >
            {d.name}
          </button>
        ))}
      </figcaption>
    </figure>
  );
}
