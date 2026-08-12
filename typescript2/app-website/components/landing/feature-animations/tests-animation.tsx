'use client';

import { motion } from 'framer-motion';
import { Check, Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';

type TestCase = {
  assertion: string;
  description: string;
  name: string;
};

const TESTS: TestCase[] = [
  {
    name: 'password_reset',
    description: 'I cannot reset my password',
    assertion: 'ticket.priority == "medium"',
  },
  {
    name: 'billing_dispute',
    description: 'I was charged twice',
    assertion: 'ticket.category == "billing"',
  },
  {
    name: 'urgent_outage',
    description: 'Production is down!',
    assertion: 'ticket.priority == "high"',
  },
];

type Phase = 'pending' | 'running' | 'passing';

const PHASE_DURATION = 950;
const SUMMARY_DELAY = 800;
const RESET_HOLD = 1800;

export function TestsAnimation({ tint }: { tint: string }) {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const total = TESTS.length * 2 + 2;
    const interval = setInterval(() => {
      setTick((t) => (t + 1) % total);
    }, PHASE_DURATION);
    return () => clearInterval(interval);
  }, []);

  const phases: Phase[] = TESTS.map((_, i) => {
    if (tick < i * 2 + 1) {
      return 'pending';
    }
    if (tick === i * 2 + 1) {
      return 'running';
    }
    return 'passing';
  });

  const showSummary = tick >= TESTS.length * 2 + 1;

  return (
    <div className="flex h-full flex-col gap-2.5">
      <div
        className="flex-1 overflow-hidden rounded-md border bg-[#1A1612] p-3 font-mono text-[11px] leading-[1.55] shadow-[0_12px_30px_-22px_rgba(26,22,18,0.55)]"
        style={{ borderColor: `${tint}55` }}
      >
        <div className="mb-1.5 flex items-center justify-between">
          <span className="font-mono text-[9px] uppercase tracking-[0.16em] text-[#8A8580]">
            triage.baml
          </span>
          <span
            className="font-mono text-[9px] uppercase tracking-[0.16em]"
            style={{ color: tint }}
          >
            testset
          </span>
        </div>
        <div className="text-[#E8DFCF]">
          <span className="font-semibold text-[#C792EA]">testset</span>{' '}
          <span className="text-[#E8B86D]">&quot;triage&quot;</span>{' '}
          <span>&#123;</span>
        </div>
        <div className="ml-3 mt-1.5 flex flex-col gap-2">
          {TESTS.map((test, i) => (
            <TestRow
              assertion={test.assertion}
              description={test.description}
              key={test.name}
              name={test.name}
              phase={phases[i]}
              tint={tint}
            />
          ))}
        </div>
        <div className="mt-1.5 text-[#E8DFCF]">&#125;</div>
      </div>

      <SummaryBar tint={tint} visible={showSummary} />
    </div>
  );
}

function TestRow({
  assertion,
  description,
  name,
  phase,
  tint,
}: {
  assertion: string;
  description: string;
  name: string;
  phase: Phase;
  tint: string;
}) {
  return (
    <div className="flex items-start gap-2">
      <div className="mt-0.5 grid size-4 shrink-0 place-items-center">
        <PhaseIcon phase={phase} tint={tint} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-1.5">
          <span className="font-semibold text-[#C792EA]">test</span>
          <span className="text-[#7FD3C4]">{name}</span>
          <span className="text-[#9B8E80]">·</span>
          <span className="truncate text-[10.5px] text-[#9B8E80]">
            {description}
          </span>
        </div>
        <motion.div
          animate={{
            opacity: phase === 'pending' ? 0 : 1,
            x: phase === 'pending' ? -4 : 0,
          }}
          className="mt-0.5 truncate text-[10.5px] text-[#E8DFCF]"
          transition={{ duration: 0.25 }}
        >
          <span className="font-semibold text-[#82AAFF]">assert</span>{' '}
          <span>{assertion}</span>
        </motion.div>
      </div>
    </div>
  );
}

function PhaseIcon({ phase, tint }: { phase: Phase; tint: string }) {
  if (phase === 'pending') {
    return (
      <span className="size-2 rounded-full border border-[#3A332B] bg-transparent" />
    );
  }
  if (phase === 'running') {
    return (
      <motion.span
        animate={{ rotate: 360 }}
        className="grid size-3 place-items-center"
        style={{ color: tint }}
        transition={{
          duration: 0.8,
          ease: 'linear',
          repeat: Number.POSITIVE_INFINITY,
        }}
      >
        <Loader2 aria-hidden size={11} strokeWidth={2.4} />
      </motion.span>
    );
  }
  return (
    <motion.span
      animate={{ scale: 1, opacity: 1 }}
      className="grid size-3.5 place-items-center rounded-full"
      initial={{ scale: 0.4, opacity: 0 }}
      style={{ backgroundColor: `${tint}33`, color: tint }}
      transition={{ duration: 0.25, ease: [0.34, 1.56, 0.64, 1] }}
    >
      <Check aria-hidden size={9} strokeWidth={3} />
    </motion.span>
  );
}

function SummaryBar({ tint, visible }: { tint: string; visible: boolean }) {
  return (
    <motion.div
      animate={{
        opacity: visible ? 1 : 0.2,
        y: visible ? 0 : 6,
      }}
      className="flex items-center justify-between rounded-md border bg-[#FFFCF6] px-3 py-2"
      style={{ borderColor: `${tint}40` }}
      transition={{ duration: 0.32 }}
    >
      <div className="flex items-center gap-2">
        <span
          className="grid size-6 place-items-center rounded-full"
          style={{ backgroundColor: `${tint}18`, color: tint }}
        >
          <Check aria-hidden size={13} strokeWidth={2.6} />
        </span>
        <div>
          <div
            className="font-mono text-[9px] uppercase tracking-[0.16em]"
            style={{ color: tint }}
          >
            assertions
          </div>
          <div className="text-[12px] font-semibold leading-tight text-[#1A1612]">
            {visible ? '3 / 3 passing' : 'running…'}
          </div>
        </div>
      </div>
      <span className="font-mono text-[10px] text-[#5C5852]">12ms</span>
    </motion.div>
  );
}
