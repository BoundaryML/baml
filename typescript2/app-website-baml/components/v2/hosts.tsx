'use client';

import { useEffect, useRef, useState } from 'react';
import { track } from '../track';

// Incremental adoption, drawn literally: one app in one process, with its files turning into BAML one at
// a time while everything else stays as it was. The file names are illustrative.
const HOSTS = [
  { command: 'python app.py', ext: 'py', label: 'Python' },
  { command: 'node app.js', ext: 'ts', label: 'Node' },
  { command: 'cargo run', ext: 'rs', label: 'Rust' },
  { command: 'npm run dev', ext: 'tsx', label: 'The web' },
];

// Six columns wide. A tile with `order` is an AI function, and becomes BAML at that step.
const TILES: {
  cols: string;
  name: string;
  order?: number;
  rows?: string;
}[] = [
  { cols: 'col-span-3', name: 'api', rows: 'row-span-2' },
  { cols: 'col-span-3', name: 'classify', order: 1 },
  { cols: 'col-span-2', name: 'billing' },
  { cols: 'col-span-1', name: 'db' },
  { cols: 'col-span-2', name: 'auth' },
  { cols: 'col-span-2', name: 'extract', order: 2 },
  { cols: 'col-span-2', name: 'search' },
  { cols: 'col-span-3', name: 'reply', order: 3 },
  { cols: 'col-span-3', name: 'models' },
];
const AI_FILES = 3;
const STEP_MS = 2400;

export default function Hosts() {
  const [host, setHost] = useState(0);
  const [adopted, setAdopted] = useState(1);
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
        if (!e.isIntersecting) return;
        id = setInterval(
          () =>
            setAdopted((a) => {
              if (a < AI_FILES) return a + 1;
              setHost((h) => (h + 1) % HOSTS.length);
              return 1;
            }),
          STEP_MS,
        );
      },
      { threshold: 0.2 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      clearInterval(id);
    };
  }, [held]);

  const { command, ext } = HOSTS[host];

  return (
    <div
      className="rounded-xl border border-white/13 overflow-hidden bg-black"
      ref={ref}
    >
      <div className="flex flex-wrap gap-1 p-2 border-b border-white/13">
        {HOSTS.map((h, i) => (
          <button
            aria-pressed={i === host}
            className={`focus-ring px-3 py-1 rounded-[3px] text-sm cursor-pointer transition-colors ${i === host ? 'bg-white/12 text-white' : 'text-white/55 hover:text-white'}`}
            key={h.label}
            onClick={() => {
              track('adoption_host_selected', { host: h.label });
              setHost(i);
              setHeld(true);
            }}
            type="button"
          >
            {h.label}
          </button>
        ))}
      </div>

      <div className="grid grid-cols-6 auto-rows-[52px] md:auto-rows-[64px] gap-1 p-2 font-mono text-[11px] md:text-[12.5px]">
        {TILES.map((t) => {
          const on = t.order !== undefined && t.order <= adopted;
          return (
            <div
              className={`relative min-w-0 overflow-hidden rounded-[3px] ${t.cols} ${t.rows ?? ''} bg-white/6`}
              key={t.name}
            >
              {/* Fills in pixel steps, in the same voice as the headline. */}
              <span
                className={`v2-adopt absolute inset-0 bg-clover-900 border border-clover/70 ${on ? 'v2-adopt-on' : ''}`}
              />
              <span
                className={`absolute left-2.5 top-2 max-w-[calc(100%-16px)] truncate transition-colors ${on ? 'text-white' : 'text-white/45'}`}
              >
                {t.name}.{on ? 'baml' : ext}
              </span>
            </div>
          );
        })}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 px-4 py-3 border-t border-white/13 text-sm">
        <span className="font-mono text-[12.5px]">
          <span className="text-clover">$</span> {command}
        </span>
        <span className="text-white/60">
          {adopted} of {TILES.length} files in BAML, no separate container
        </span>
      </div>
    </div>
  );
}
