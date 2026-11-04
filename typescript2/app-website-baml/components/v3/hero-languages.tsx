'use client';

import { useState } from 'react';
import LoopLine from './loop-line';

// How far each language's toolchain reaches. Every row stops somewhere and leaves the rest to other
// products, and only BAML's reaches production and connects back to the start. Go can profile production,
// so its row says so, and the page doesn't claim that no language can.
const COLUMNS = ['Build', 'Test', 'Deploy', 'Monitor'];

const ROWS: {
  aside?: { column: number; text: string };
  commands: string[];
  name: string;
  year: string;
}[] = [
  { commands: ['cc'], name: 'C', year: '1972' },
  {
    aside: { column: 3, text: 'pprof, sampled' },
    commands: ['go build', 'go test'],
    name: 'Go',
    year: '2009',
  },
  {
    aside: { column: 3, text: 'Separate products' },
    commands: ['cargo build', 'cargo test', 'cargo publish'],
    name: 'Rust',
    year: '2015',
  },
];

const BAML = ['baml check', 'baml test', 'baml pack', 'baml query'];
const STATIONS = [0.125, 0.375, 0.625, 0.875];
const GRID =
  'grid grid-cols-[52px_repeat(4,minmax(0,1fr))] md:grid-cols-[132px_repeat(4,minmax(0,1fr))] gap-x-1.5 md:gap-x-2';
const BAR_HEIGHT = 'h-14 md:h-[76px]';

function Cells({
  active = -1,
  commands,
}: {
  active?: number;
  commands: string[];
}) {
  return (
    <div
      className="grid h-full"
      style={{ gridTemplateColumns: `repeat(${commands.length}, 1fr)` }}
    >
      {commands.map((command, i) => (
        <span
          className={`hidden sm:flex items-center justify-center min-w-0 px-1 font-mono text-[11.5px] md:text-[15px] whitespace-nowrap transition-colors duration-300 ${active === i ? 'text-white' : 'text-white/75'}`}
          key={command}
        >
          {command}
        </span>
      ))}
    </div>
  );
}

export default function HeroLanguages() {
  const [active, setActive] = useState(-1);
  const back = active === COLUMNS.length;

  return (
    <figure
      aria-label="How far each language's toolchain reaches. C covers build. Go covers build and test. Rust covers build, test, and deploy. BAML covers build, test, deploy, and monitor, and a line runs from monitor back to build."
      className="relative pb-24"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] top-[10%] h-[720px]"
      />
      <div className="relative flex flex-col gap-2 md:gap-2.5">
        <div className={`${GRID} mb-2 md:mb-3`}>
          <span />
          {COLUMNS.map((c, i) => (
            <p
              className={`v3-stage text-center font-display font-medium text-[15px] sm:text-[22px] md:text-[34px] leading-none tracking-[-0.04em] ${active === i ? 'v3-stage-on' : ''}`}
              key={c}
            >
              {c}
            </p>
          ))}
        </div>

        {ROWS.map((row, r) => (
          <div className={`${GRID} ${BAR_HEIGHT}`} key={row.name}>
            <div className="self-center min-w-0">
              <p className="text-[15px] md:text-xl text-white/70">{row.name}</p>
              <p className="hidden md:block mt-0.5 text-[13px] text-white/35">
                {row.year}
              </p>
            </div>
            <div
              className="v3-reach rounded-[10px]"
              style={
                {
                  '--row': r,
                  gridColumn: `2 / span ${row.commands.length}`,
                } as React.CSSProperties
              }
            >
              <Cells commands={row.commands} />
            </div>
            {row.aside && (
              <p
                className="hidden sm:flex items-center justify-center min-w-0 rounded-[10px] border border-dashed border-white/12 font-mono text-[11.5px] md:text-[13px] text-white/40 text-center"
                style={{ gridColumn: 2 + row.aside.column }}
              >
                {row.aside.text}
              </p>
            )}
          </div>
        ))}

        <div className={`${GRID} mt-2 md:mt-4`}>
          <div className={`${BAR_HEIGHT} flex items-center`}>
            <p className="v2-baml-mark text-[17px] md:text-[28px] text-white">
              BAML
            </p>
          </div>
          <div className="relative col-span-4 h-[132px] md:h-[186px]">
            <div
              className={`v3-reach-baml rounded-[10px] ${BAR_HEIGHT}`}
              style={{ '--row': ROWS.length } as React.CSSProperties}
            >
              <Cells active={active} commands={BAML} />
            </div>
            {/* Starts at the bar's lower edge, so the line runs along the bar rather than through its text. */}
            <div className="absolute inset-x-0 bottom-0 top-14 md:top-[76px]">
              <LoopLine
                active={active}
                inset={22}
                nodes={false}
                onStage={setActive}
                stations={STATIONS}
                top={0}
              />
            </div>
            <p
              className={`absolute left-1/2 bottom-[22px] -translate-x-1/2 translate-y-1/2 px-3 md:px-5 bg-black whitespace-nowrap text-[11.5px] sm:text-[13px] md:text-lg transition-colors duration-300 ${back ? 'text-clover' : 'text-white/60'}`}
            >
              Improve
            </p>
          </div>
        </div>
      </div>
    </figure>
  );
}
