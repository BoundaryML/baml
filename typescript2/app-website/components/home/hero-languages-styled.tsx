'use client';

import { Beam, Card, Frame, Lamb, useBox } from './kit';

// Version 1, "Languages and BAML", drawn with the parts from /diagram-styles so it can be shown in each
// of that board's styles. The switcher picks the style; every color, border, radius, and font comes from
// the .ds-theme-* class around it. Everything is drawn at once, and the only motion is the beam on the
// line back to build.
export const DIAGRAM_STYLES = [
  { id: 'midnight', name: 'Midnight' },
  // Defined in app/v3/v3.css rather than on the /diagram-styles board: no frame, cards, or cells.
  { id: 'bare', name: 'No boxes' },
  { id: 'clover', name: 'Clover' },
  { id: 'terminal', name: 'Terminal' },
  { id: 'ink', name: 'Ink' },
  { id: 'outline', name: 'Bold outline' },
  { id: 'glass', name: 'Glass' },
  { id: 'arcade', name: 'Arcade' },
  { id: 'mono', name: 'Mono' },
];

const COLUMNS = ['Build', 'Test', 'Deploy', 'Monitor'];

const ROWS: {
  aside?: string;
  commands: string[];
  name: string;
  year: string;
}[] = [
  { commands: ['cc'], name: 'C', year: '1972' },
  {
    aside: 'pprof, sampled',
    commands: ['go build', 'go test'],
    name: 'Go',
    year: '2009',
  },
  {
    aside: 'Separate products',
    commands: ['cargo build', 'cargo test', 'cargo publish'],
    name: 'Rust',
    year: '2015',
  },
];

const BAML = ['baml check', 'baml test', 'baml pack', 'baml query'];

const GRID =
  'grid grid-cols-[44px_repeat(4,minmax(0,1fr))] @min-[640px]:grid-cols-[120px_repeat(4,minmax(0,1fr))] gap-x-1.5 @min-[640px]:gap-x-2';
const ROW_HEIGHT = 'h-11 @min-[640px]:h-16';

export function Commands({
  commands,
  lit,
}: {
  commands: string[];
  lit?: boolean;
}) {
  return (
    <div
      className="grid h-full"
      style={{ gridTemplateColumns: `repeat(${commands.length}, 1fr)` }}
    >
      {commands.map((command) => (
        <span
          className={`hidden @min-[560px]:flex items-center justify-center min-w-0 px-1 font-mono text-[11.5px] @min-[960px]:text-[14px] whitespace-nowrap ${lit ? 'text-(--ds-accent-ink)' : 'text-(--ds-ink)/70'}`}
          key={command}
        >
          {command}
        </span>
      ))}
    </div>
  );
}

/** The line from Monitor back to Build, under the BAML row, with a beam running along it. */
function Return() {
  const [ref, { h, w }] = useBox<HTMLDivElement>();
  // Each station is the middle of its column; the corners are rounded to match the cards.
  const from = w * 0.875;
  const to = w * 0.125;
  const y = h / 2;
  const r = Math.min(12, y);
  const d = `M${from} 0V${y - r}Q${from} ${y} ${from - r} ${y}H${to + r}Q${to} ${y} ${to} ${y - r}V2`;

  return (
    <div className="relative h-14 @min-[640px]:h-20" ref={ref}>
      {w > 0 && (
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full overflow-visible"
          fill="none"
        >
          <path d={d} stroke="var(--ds-line)" strokeWidth="1.5" />
          <path
            d={`M${to - 4} 6L${to} 2L${to + 4} 6`}
            stroke="var(--ds-accent)"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="1.5"
          />
          <Beam d={d} dur={3.6} />
        </svg>
      )}
      <p className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 px-3 bg-(--ds-ground) text-[12px] @min-[640px]:text-[14px] @min-[960px]:text-[16px] text-(--ds-accent-ink) whitespace-nowrap">
        <span className="@min-[640px]:hidden">Back into the next build</span>
        <span className="hidden @min-[640px]:inline">Improve</span>
      </p>
    </div>
  );
}

export default function HeroLanguagesStyled({
  style = 'midnight',
}: {
  style?: string;
}) {
  return (
    <div
      className={`ds-theme ds-theme-${style} rounded-[28px] p-2 sm:p-4 md:p-5`}
    >
      <Frame label="How far each language's toolchain reaches. C covers build. Go covers build and test, and samples production with pprof. Rust covers build, test, and deploy. BAML covers build, test, deploy, and monitor, and a line runs from monitor back to build.">
        <div className="p-4 @min-[640px]:p-8 @min-[960px]:p-12">
          <div className={`${GRID} mb-3 @min-[640px]:mb-4`}>
            <span />
            {COLUMNS.map((c) => (
              <p
                className="text-center font-display font-medium text-[14px] @min-[640px]:text-[22px] @min-[960px]:text-[30px] leading-none tracking-[-0.03em]"
                key={c}
              >
                {c}
              </p>
            ))}
          </div>

          <div className="flex flex-col gap-1.5 @min-[640px]:gap-2">
            {ROWS.map((row) => (
              <div className={`${GRID} ${ROW_HEIGHT}`} key={row.name}>
                <div className="self-center min-w-0">
                  <p className="text-[14px] @min-[640px]:text-[17px] text-(--ds-ink)/75">
                    {row.name}
                  </p>
                  <p className="hidden @min-[640px]:block font-mono text-[12px] text-(--ds-ink)/35">
                    {row.year}
                  </p>
                </div>
                <Card
                  className="min-w-0"
                  style={{ gridColumn: `2 / span ${row.commands.length}` }}
                >
                  <Commands commands={row.commands} />
                </Card>
                {row.aside && (
                  <p
                    className="hidden @min-[560px]:flex items-center justify-center min-w-0 px-1 border border-dashed border-(--ds-line) rounded-(--ds-card-radius) font-mono text-[11px] @min-[960px]:text-[13px] text-(--ds-ink)/40 text-center"
                    style={{ gridColumn: 5 }}
                  >
                    {row.aside}
                  </p>
                )}
              </div>
            ))}
          </div>

          <div className={`${GRID} mt-3 @min-[640px]:mt-5`}>
            <div className={`${ROW_HEIGHT} flex items-center gap-2 min-w-0`}>
              <span className="hidden @min-[640px]:block">
                <Lamb size={22} />
              </span>
              <p className="font-display font-medium text-[14px] @min-[640px]:text-[19px] tracking-[-0.02em]">
                BAML
              </p>
            </div>
            <div className="col-span-4 min-w-0">
              <Card
                className={ROW_HEIGHT}
                lit
                style={{
                  backgroundColor: 'rgb(from var(--ds-accent) r g b / 0.1)',
                }}
              >
                <Commands commands={BAML} lit />
              </Card>
              <Return />
            </div>
          </div>
        </div>
      </Frame>
    </div>
  );
}
