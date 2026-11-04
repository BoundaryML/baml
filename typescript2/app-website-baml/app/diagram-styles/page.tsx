import type { Metadata } from 'next';
import { Instrument_Serif } from 'next/font/google';
import localFont from 'next/font/local';
import type { ReactNode } from 'react';
import Agent from '@/components/diagram-styles/agent';
import Bento from '@/components/diagram-styles/bento';
import Heatmap from '@/components/diagram-styles/heatmap';
import Layers from '@/components/diagram-styles/layers';
import Network from '@/components/diagram-styles/network';
import Orbit from '@/components/diagram-styles/orbit';
import Pipeline from '@/components/diagram-styles/pipeline';
import Routing from '@/components/diagram-styles/routing';
import Schema from '@/components/diagram-styles/schema';
import Waterfall from '@/components/diagram-styles/waterfall';
import ForceDark from '@/components/v2/force-dark';
import Nav from '@/components/v2/nav';
import '../v2/v2.css';
import './styles.css';

// Geist Pixel is BAML's voice in the brand: the page headline, and numbers in Bold outline and Arcade.
const pixel = localFont({
  display: 'swap',
  src: '../fonts/GeistPixel-Square.otf',
  variable: '--font-geist-pixel',
});

// The serif for the Ink style.
const serif = Instrument_Serif({
  display: 'swap',
  subsets: ['latin'],
  variable: '--font-instrument-serif',
  weight: '400',
});

// A board of diagram styles for the site, to pick from before drawing the real ones. Each style is a
// theme in styles.css, and each shows a board of diagrams at once. Its copy is in copy/diagram-styles.md.
export const metadata: Metadata = {
  robots: { follow: false, index: false },
  title: 'BAML | Diagram styles',
};

type Cell = { diagram: ReactNode; wide?: boolean };

const STYLES: { board: Cell[]; id: string; name: string; note: string }[] = [
  {
    board: [
      { diagram: <Network />, wide: true },
      { diagram: <Agent /> },
      { diagram: <Heatmap /> },
      { diagram: <Waterfall />, wide: true },
      { diagram: <Pipeline /> },
      { diagram: <Orbit /> },
    ],
    id: 'midnight',
    name: 'Midnight',
    note: 'The current style. Near-black panels, hairline borders, and clover as the only accent.',
  },
  {
    board: [
      { diagram: <Orbit />, wide: true },
      { diagram: <Heatmap /> },
      { diagram: <Agent /> },
      { diagram: <Network />, wide: true },
      { diagram: <Layers /> },
      { diagram: <Routing /> },
    ],
    id: 'clover',
    name: 'Clover',
    note: 'Set on the brand’s darkest clover, with pasture green as the accent. For a section that should look like BAML and no other dark site.',
  },
  {
    board: [
      { diagram: <Pipeline />, wide: true },
      { diagram: <Agent /> },
      { diagram: <Waterfall /> },
      { diagram: <Schema />, wide: true },
      { diagram: <Heatmap /> },
      { diagram: <Routing /> },
    ],
    id: 'terminal',
    name: 'Terminal',
    note: 'One monospace face, square corners, dashed frames, and a line grid. It reads like a tool that lives in your terminal.',
  },
  {
    board: [
      { diagram: <Routing />, wide: true },
      { diagram: <Layers /> },
      { diagram: <Schema /> },
      { diagram: <Bento />, wide: true },
      { diagram: <Heatmap /> },
      { diagram: <Waterfall /> },
    ],
    id: 'ink',
    name: 'Ink',
    note: 'Warm charcoal, paper-white type, and a serif for headings and numbers, with hairlines and no shadows. Quiet and exact, like the figures in a research paper.',
  },
  {
    board: [
      { diagram: <Bento />, wide: true },
      { diagram: <Pipeline /> },
      { diagram: <Agent /> },
      { diagram: <Network />, wide: true },
      { diagram: <Orbit /> },
      { diagram: <Heatmap /> },
    ],
    id: 'outline',
    name: 'Bold outline',
    note: 'Thick outlines, hard offset shadows in clover, and Geist Pixel numbers. Loud on purpose.',
  },
  {
    board: [
      { diagram: <Orbit />, wide: true },
      { diagram: <Agent /> },
      { diagram: <Routing /> },
      { diagram: <Network />, wide: true },
      { diagram: <Schema /> },
      { diagram: <Heatmap /> },
    ],
    id: 'glass',
    name: 'Glass',
    note: 'Frosted panels over the brand’s dusk painting. Soft and atmospheric, for a hero or a section opener.',
  },
  {
    board: [
      { diagram: <Pipeline />, wide: true },
      { diagram: <Heatmap /> },
      { diagram: <Orbit /> },
      { diagram: <Waterfall />, wide: true },
      { diagram: <Agent /> },
      { diagram: <Layers /> },
    ],
    id: 'arcade',
    name: 'Arcade',
    note: 'Geist Pixel for numbers and code, square corners, two-pixel borders, and hard shadows, with pasture green as the accent.',
  },
  {
    board: [
      { diagram: <Network />, wide: true },
      { diagram: <Pipeline /> },
      { diagram: <Routing /> },
      { diagram: <Bento />, wide: true },
      { diagram: <Agent /> },
      { diagram: <Waterfall /> },
    ],
    id: 'mono',
    name: 'Mono',
    note: 'Black and white only. Red is the one color, and only for failure. The plainest of the set.',
  },
];

export default function DiagramStyles() {
  return (
    <div
      className={`v2 min-h-screen ${pixel.variable} ${serif.variable}`}
      data-theme="dark"
    >
      <ForceDark />
      <Nav />
      <main id="main">
        <header className="max-w-[1280px] mx-auto px-5 md:px-12 pt-14 md:pt-24 pb-16 md:pb-20">
          <h1 className="v2-h1">Diagram styles</h1>
          <p className="max-w-[42rem] mt-6 text-lg md:text-xl leading-[1.45] text-white/70">
            Eight dark styles for the BAML site’s diagrams. Each style shows a
            board of diagrams at once, built from the same parts, so the styles
            can be compared side by side. All data is example data.
          </p>
          <nav aria-label="Styles" className="flex flex-wrap gap-2 mt-8">
            {STYLES.map((s) => (
              <a
                className="focus-ring px-3 py-1.5 rounded-full border border-white/15 text-[14px] text-white/70 hover:text-white hover:border-white/40 transition-colors"
                href={`#${s.id}`}
                key={s.id}
              >
                {s.name}
              </a>
            ))}
          </nav>
        </header>

        {STYLES.map((s) => (
          <section
            aria-labelledby={`${s.id}-title`}
            className={`ds-theme ds-theme-${s.id} scroll-mt-16`}
            id={s.id}
            key={s.id}
          >
            <div className="max-w-[1280px] mx-auto px-5 md:px-12 py-16 md:py-24">
              <div className="flex flex-wrap items-baseline justify-between gap-x-10 gap-y-2 mb-8 md:mb-10">
                <h2
                  className="font-display font-medium text-[32px] md:text-[48px] leading-none tracking-[-0.03em]"
                  id={`${s.id}-title`}
                >
                  {s.name}
                </h2>
                <p className="max-w-[36rem] text-[15px] md:text-base leading-[1.5] opacity-60">
                  {s.note}
                </p>
              </div>
              <div className="grid lg:grid-cols-2 gap-4 md:gap-6">
                {s.board.map((c, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: a cell is a fixed slot in the board
                  <div
                    className={`ds-cell min-w-0 ${c.wide ? 'lg:col-span-2' : ''}`}
                    key={i}
                  >
                    {c.diagram}
                  </div>
                ))}
              </div>
            </div>
          </section>
        ))}
      </main>
    </div>
  );
}
