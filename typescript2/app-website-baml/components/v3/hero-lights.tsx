'use client';

import { useEffect, useState } from 'react';
import { RED } from './story';

// The same deploy, seen by the agent that made it. In any other language the screen goes dark once the
// code ships. In BAML the calls keep coming back. The calls are example data.
const TICK_MS = 450;
const TICKS = 20;
const DEPLOY = 4;

const CALLS = [
  { input: 'App crashes when I upload a photo', label: 'bug', ms: 612 },
  { input: 'How do I change my plan?', label: 'billing', ms: 540 },
  {
    input: 'Love waiting 3 weeks for a refund',
    label: 'other',
    ms: 701,
    wrong: true,
  },
  { input: '¿Dónde está mi factura?', label: 'billing', ms: 588 },
  { input: 'Export button does nothing', label: 'bug', ms: 634 },
  { input: 'Great, charged twice again', label: 'other', ms: 677, wrong: true },
  { input: 'Can I get an invoice for March?', label: 'billing', ms: 520 },
  { input: 'Login loops back to the start', label: 'bug', ms: 649 },
];

function Panel({
  children,
  dark,
  lit,
  title,
  version,
  deploying,
}: {
  children?: React.ReactNode;
  dark?: boolean;
  deploying: boolean;
  lit?: boolean;
  title: string;
  version: number;
}) {
  return (
    <div className="min-w-0">
      <p className="mb-4 text-[15px] md:text-lg text-white/70">{title}</p>
      <div
        className={`v3-window relative h-[320px] md:h-[380px] overflow-hidden transition-[background-color,box-shadow,border-color] duration-700 ${dark && !deploying ? 'v3-off' : ''} ${lit && !deploying ? 'v3-card-on' : ''}`}
      >
        <div
          className={`flex items-center justify-between h-11 px-4 border-b border-white/8 text-[13px] transition-opacity duration-700 ${dark && !deploying ? 'opacity-25' : ''}`}
        >
          <span className="text-white/70">triage</span>
          <span className="font-mono text-white/40">v{version}</span>
        </div>
        {deploying ? (
          <div className="v3-enter grid place-items-center h-[calc(100%-44px)] px-8">
            <div className="w-full max-w-64">
              <p className="mb-3 text-[14px] text-white/70">
                Deploying v{version}
              </p>
              <div className="h-1 rounded-full bg-white/10 overflow-hidden">
                <div
                  className="v3-cycle h-full bg-clover origin-left"
                  style={
                    { '--ms': `${DEPLOY * TICK_MS}ms` } as React.CSSProperties
                  }
                />
              </div>
            </div>
          </div>
        ) : (
          children
        )}
      </div>
    </div>
  );
}

export default function HeroLights() {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setTick(DEPLOY + 7);
      return;
    }
    const id = setInterval(() => setTick((t) => t + 1), TICK_MS);
    return () => clearInterval(id);
  }, []);

  const lap = Math.floor(tick / TICKS);
  const t = tick % TICKS;
  const deploying = t < DEPLOY;
  const version = 42 + lap;
  const seen = Math.max(0, t - DEPLOY + 1);
  const rows = Array.from({ length: Math.min(seen, 8) }, (_, i) => {
    const n = lap * 16 + seen - 1 - i;
    return { ...CALLS[n % CALLS.length], id: n };
  });

  return (
    <figure
      aria-label="The same deploy in two languages. In any other language, the agent's view goes dark once the code ships. In BAML, the agent keeps seeing every production call, with its input, label, and latency."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className="v3-glow left-[30%] right-[-20%] -top-16 h-[640px]"
      />
      <div className="relative grid md:grid-cols-2 gap-10 md:gap-6">
        <Panel
          dark
          deploying={deploying}
          title="What your agent sees, in any other language"
          version={version}
        >
          <div className="v3-enter grid place-items-center h-[calc(100%-44px)] px-8 text-center">
            <p className="text-[15px] md:text-lg text-white/70 text-balance">
              Deployed. Your agent can't see what happens next.
            </p>
          </div>
        </Panel>
        <Panel
          deploying={deploying}
          lit
          title="What your agent sees, in BAML"
          version={version}
        >
          <ul className="flex flex-col px-4 py-2 font-mono text-[12px] md:text-[13px]">
            {rows.map((r, i) => (
              <li
                className={`flex items-center gap-3 h-9 border-b border-white/6 ${i === 0 ? 'v3-land' : ''}`}
                key={r.id}
              >
                <span className="flex-1 min-w-0 truncate text-[#BDBDC4]">
                  {r.input}
                </span>
                <span
                  className="shrink-0 w-16 text-right"
                  style={{ color: r.wrong ? RED : 'var(--color-clover)' }}
                >
                  {r.label}
                </span>
                <span className="hidden sm:block shrink-0 w-14 text-right text-white/35">
                  {r.ms}ms
                </span>
              </li>
            ))}
          </ul>
        </Panel>
      </div>
    </figure>
  );
}
