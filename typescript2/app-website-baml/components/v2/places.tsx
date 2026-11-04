'use client';

import { useEffect, useRef, useState } from 'react';
import { track } from '../track';

// One query that stays put while the place it runs changes around it. On a laptop the answer is a table
// from files in the project; in Boundary the same answer feeds dashboards, metrics, and alerts. The
// query uses the tables baml query reads; the result rows are example data.
const PLACES = [
  { key: 'laptop', label: 'Your laptop', sub: 'files in your project' },
  { key: 'boundary', label: 'Boundary', sub: 'hosted, or in your AWS' },
];

const RESULT: [string, string, string, number][] = [
  ['user.ExtractInvoice', '8412', '401', 100],
  ['user.classify', '2960', '188', 35],
  ['user.reply', '1204', '266', 14],
];

const FLIP_MS = 4200;

function Table() {
  return (
    <table className="w-full text-left font-mono text-[12px] md:text-[13px] leading-6">
      <thead className="text-white/50">
        <tr>
          <th className="font-normal" scope="col">
            span_name
          </th>
          <th className="font-normal text-right" scope="col">
            calls
          </th>
          <th className="font-normal text-right" scope="col">
            avg_ms
          </th>
        </tr>
      </thead>
      <tbody className="text-white/90">
        {RESULT.map(([name, calls, ms]) => (
          <tr className="border-t border-white/10" key={name}>
            <td className="py-1.5">{name}</td>
            <td className="text-right">{calls}</td>
            <td className="text-right">{ms}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Tools({ on }: { on: boolean }) {
  return (
    <div className="grid grid-cols-[1.5fr_1fr] gap-2 text-[13px]">
      <div className="row-span-2 rounded-[6px] border border-white/12 p-3">
        <div className="text-white/60">Dashboard</div>
        <div className="flex flex-col gap-2.5 mt-3">
          {RESULT.map(([name, calls, , width], i) => (
            <div key={name}>
              <div className="flex justify-between font-mono text-[11px] text-white/70">
                <span className="truncate">{name.replace('user.', '')}</span>
                <span>{calls}</span>
              </div>
              <div className="h-1.5 mt-1 rounded-full bg-white/8">
                <div
                  className="h-full rounded-full bg-pasture-200 origin-left transition-transform duration-700"
                  style={{
                    transform: on ? 'scaleX(1)' : 'scaleX(0)',
                    transitionDelay: `${i * 90}ms`,
                    width: `${width}%`,
                  }}
                />
              </div>
            </div>
          ))}
        </div>
      </div>
      <div className="rounded-[6px] border border-white/12 p-3">
        <div className="text-white/60">Metric</div>
        <div className="mt-1 font-display text-[22px] leading-7 tracking-[-0.03em]">
          401 ms
        </div>
      </div>
      <div className="rounded-[6px] border border-white/12 p-3">
        <div className="flex items-center gap-1.5 text-white/60">
          <span className="size-1.5 rounded-full bg-[#E5484D]" />
          Alert
        </div>
        <div className="mt-1 truncate font-mono text-[11px] leading-5 text-white/85">
          avg_ms &gt; 400
        </div>
      </div>
    </div>
  );
}

export default function Places() {
  const [place, setPlace] = useState(0);
  const [held, setHeld] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (held) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const el = ref.current;
    if (!el) return;
    let id: ReturnType<typeof setInterval> | undefined;
    const io = new IntersectionObserver(
      ([e]) => {
        clearInterval(id);
        if (e.isIntersecting)
          id = setInterval(() => setPlace((p) => (p + 1) % 2), FLIP_MS);
      },
      { threshold: 0.3 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      clearInterval(id);
    };
  }, [held]);

  const cloud = place === 1;

  return (
    <div
      className="min-w-0 rounded-2xl border border-white/20 bg-black/88 backdrop-blur-sm overflow-hidden shadow-[0_40px_100px_-30px_rgb(0_0_0/0.9)]"
      ref={ref}
    >
      <div className="p-2 border-b border-white/12">
        <div className="relative grid grid-cols-2 rounded-[10px] bg-white/6">
          <span
            className={`absolute inset-y-0 left-0 w-1/2 rounded-[10px] transition-[transform,background-color] duration-300 ${cloud ? 'translate-x-full bg-pasture-800' : 'bg-clover-900'}`}
          />
          {PLACES.map((p, i) => (
            <button
              aria-pressed={i === place}
              className="focus-ring relative px-3 py-2.5 text-left cursor-pointer"
              key={p.key}
              onClick={() => {
                track('query_place_selected', { place: p.key });
                setPlace(i);
                setHeld(true);
              }}
              type="button"
            >
              <span className="block text-[15px] leading-5">{p.label}</span>
              <span className="block text-[12.5px] leading-5 text-white/65">
                {p.sub}
              </span>
            </button>
          ))}
        </div>
      </div>

      <pre className="px-5 pt-5 overflow-x-auto font-mono text-[12px] md:text-[13px] leading-[21px] text-[#D8DDD3]">
        <span className="text-white/45">$ baml query </span>
        {'\n'}
        <span className="text-clover">SELECT</span> span_name, count(*){' '}
        <span className="text-clover">AS</span> calls,{'\n'}
        {'  '}avg(duration) / 1e6 <span className="text-clover">AS</span> avg_ms
        {'\n'}
        <span className="text-clover">FROM</span> spans{' '}
        <span className="text-clover">GROUP BY</span> span_name
      </pre>

      {/* Where the data comes from. The recordings only travel in the Boundary state. */}
      <div className="flex items-center gap-3 px-5 mt-4 font-mono text-[11.5px] text-white/60">
        <span>.baml/btel</span>
        <svg
          aria-hidden="true"
          className={`v2-pulse-green flex-1 min-w-6 h-1.5 transition-opacity duration-300 ${cloud ? 'opacity-100' : 'opacity-0'}`}
          preserveAspectRatio="none"
          viewBox="0 0 240 6"
        >
          <path
            className="v2-march"
            d="M0 3H240"
            vectorEffect="non-scaling-stroke"
          />
        </svg>
        <span className={`transition-opacity ${cloud ? '' : 'opacity-0'}`}>
          Boundary
        </span>
      </div>

      <div className="grid p-5 pt-4">
        <div
          aria-hidden={cloud}
          className={`[grid-area:1/1] transition-[opacity,transform] duration-300 ${cloud ? 'opacity-0 -translate-x-3 pointer-events-none' : ''}`}
        >
          <Table />
        </div>
        <div
          aria-hidden={!cloud}
          className={`[grid-area:1/1] transition-[opacity,transform] duration-300 ${cloud ? '' : 'opacity-0 translate-x-3 pointer-events-none'}`}
        >
          <Tools on={cloud} />
        </div>
      </div>
      <p className="px-5 pb-4 text-[11px] leading-4 text-white/45">
        Example data
      </p>
    </div>
  );
}
