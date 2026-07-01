'use client';

import Image from 'next/image';
import { useAnimateInView } from '../../learn3/_lib/use-animate-in-view';

/*
 * The benchmark tables as horizontal bars. Bars grow in on scroll (width
 * animation — not scaleX, so the sheep riding the end of the BAML bars
 * doesn't distort). All numbers measured 2026-06-10 on an idle 18-core
 * Apple Silicon machine, Bun 1.3.14 vs the release toolchain.
 */

interface ChartRow {
  name: string;
  /** Bar length as a fraction of the group's max. */
  frac: number;
  value: string;
  sub?: string;
  baml?: boolean;
}

interface ChartGroup {
  label: string;
  rows: ChartRow[];
}

function BenchChart({ groups }: { groups: ChartGroup[] }) {
  const { ref, holdClass } = useAnimateInView();
  let barIndex = 0;
  return (
    <div className={`l6-chart${holdClass}`} ref={ref}>
      {groups.map((group) => (
        <div className="l6-chart-group" key={group.label}>
          <p className="l6-chart-cap font-mono">{group.label}</p>
          {group.rows.map((row) => {
            const delay = `${barIndex++ * 0.18}s`;
            return (
              <div className="l6-chart-row" key={`${group.label}-${row.name}`}>
                <span className="l6-chart-name font-mono">{row.name}</span>
                <span className="l6-chart-track">
                  <span
                    className={`l6-chart-fill${row.baml ? ' l6-chart-fill--baml' : ''}`}
                    style={
                      {
                        '--w': `${(row.frac * 100).toFixed(1)}%`,
                        animationDelay: delay,
                      } as React.CSSProperties
                    }
                  >
                    {row.baml && (
                      <Image
                        alt=""
                        className="l6-chart-sheep"
                        height={26}
                        src="/baml-sheep.png"
                        width={26}
                      />
                    )}
                  </span>
                  <span
                    className="l6-chart-val font-mono"
                    style={{ animationDelay: `calc(${delay} + 0.55s)` }}
                  >
                    {row.value}
                    {row.sub ? (
                      <i className="l6-chart-sub">{` · ${row.sub}`}</i>
                    ) : null}
                  </span>
                </span>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/* `baml pack` vs `bun build --compile`: same hello world, binary size. */
const PACK_GROUPS: ChartGroup[] = [
  {
    label: 'binary size',
    rows: [
      {
        baml: true,
        frac: 12.1 / 63.1,
        name: 'baml pack',
        sub: '5.7 MB gzipped',
        value: '12.1 MB',
      },
      {
        frac: 1,
        name: 'bun build --compile',
        sub: '23.5 MB gzipped',
        value: '63.1 MB',
      },
    ],
  },
];

export function PackChart() {
  return <BenchChart groups={PACK_GROUPS} />;
}

/* The 38.4 GB text-scan benchmark (16 shards × 50 rounds × ~48 MB). */
const SPAWN_GROUPS: ChartGroup[] = [
  {
    label: 'scan 38.4 GB of text · wall clock',
    rows: [
      { frac: 1, name: 'bun, one thread', sub: '1 core', value: '8.2 s' },
      {
        baml: true,
        frac: 6.8 / 8.2,
        name: 'baml, one thread',
        sub: '1 core',
        value: '6.8 s',
      },
      {
        baml: true,
        frac: 0.87 / 8.2,
        name: 'baml, spawn ×16',
        sub: '10 cores',
        value: '0.87 s',
      },
    ],
  },
];

export function SpawnChart() {
  return <BenchChart groups={SPAWN_GROUPS} />;
}
