'use client';

import { AnimatePresence, motion } from 'framer-motion';
import { useEffect, useState } from 'react';

type Substitution = {
  arrayLiteral: string;
  color: string;
  result: string;
  type: string;
};

const SUBSTITUTIONS: Substitution[] = [
  {
    type: 'string',
    color: '#7FD3C4',
    arrayLiteral: '["alpha", "beta", "gamma"]',
    result: '"alpha"',
  },
  {
    type: 'Receipt',
    color: '#82AAFF',
    arrayLiteral: '[r1, r2, r3]',
    result: 'r1: Receipt',
  },
  {
    type: 'int',
    color: '#E8B86D',
    arrayLiteral: '[42, 13, 7]',
    result: '42',
  },
];

export function GenericsAnimation({ tint }: { tint: string }) {
  const [idx, setIdx] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setIdx((i) => (i + 1) % SUBSTITUTIONS.length);
    }, 1900);
    return () => clearInterval(interval);
  }, []);

  const sub = SUBSTITUTIONS[idx];

  return (
    <div className="flex h-full flex-col gap-3">
      <div
        className="relative rounded-md border bg-[#1A1612] p-3 font-mono text-[11px] leading-[1.55] shadow-[0_12px_30px_-22px_rgba(26,22,18,0.55)]"
        style={{ borderColor: `${tint}55` }}
      >
        <div className="mb-1.5 flex items-center justify-between">
          <span className="font-mono text-[9px] uppercase tracking-[0.16em] text-[#8A8580]">
            first.baml
          </span>
          <span
            className="font-mono text-[9px] uppercase tracking-[0.16em]"
            style={{ color: tint }}
          >
            generic
          </span>
        </div>
        <div className="text-[#E8DFCF]">
          <div>
            <span className="font-semibold text-[#C792EA]">function</span>{' '}
            <span className="text-[#7FD3C4]">First</span>
            <span className="text-[#9B8E80]">&lt;</span>
            <TypeSlot color={sub.color} type={sub.type} />
            <span className="text-[#9B8E80]">&gt;</span>
            <span>(items: </span>
            <TypeSlot color={sub.color} type={sub.type} />
            <span>[]) -&gt; </span>
            <TypeSlot color={sub.color} type={sub.type} />
            <span>? &#123;</span>
          </div>
          <div className="ml-4">
            items.<span className="text-[#82AAFF]">at</span>(
            <span className="text-[#E8B86D]">0</span>)
          </div>
          <div>&#125;</div>
        </div>
      </div>

      <SubstitutionGlow tint={tint} />

      <div
        className="rounded-md border bg-[#1A1612] p-3 font-mono text-[11px] leading-[1.55] shadow-[0_12px_30px_-22px_rgba(26,22,18,0.55)]"
        style={{ borderColor: `${tint}55` }}
      >
        <div className="mb-1.5 font-mono text-[9px] uppercase tracking-[0.16em] text-[#8A8580]">
          call site
        </div>
        <AnimatePresence mode="wait">
          <motion.div
            animate={{ opacity: 1, y: 0 }}
            className="text-[#E8DFCF]"
            exit={{ opacity: 0, y: -6 }}
            initial={{ opacity: 0, y: 8 }}
            key={sub.type}
            transition={{ duration: 0.3, ease: [0.22, 0.61, 0.36, 1] }}
          >
            <div>
              <span className="text-[#7FD3C4]">First</span>
              <span className="text-[#9B8E80]">&lt;</span>
              <span className="font-semibold" style={{ color: sub.color }}>
                {sub.type}
              </span>
              <span className="text-[#9B8E80]">&gt;</span>
              <span>(</span>
              <span className="text-[#E8B86D]">{sub.arrayLiteral}</span>
              <span>)</span>
            </div>
            <div className="mt-1 flex items-center gap-2">
              <span className="text-[#9B8E80]">↳</span>
              <span className="font-semibold" style={{ color: sub.color }}>
                {sub.result}
              </span>
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}

function TypeSlot({ color, type }: { color: string; type: string }) {
  return (
    <span className="relative inline-block">
      <AnimatePresence mode="wait">
        <motion.span
          animate={{ opacity: 1, y: 0 }}
          className="inline-block font-semibold"
          exit={{ opacity: 0, y: -6 }}
          initial={{ opacity: 0, y: 8 }}
          key={type}
          style={{ color }}
          transition={{ duration: 0.25, ease: [0.22, 0.61, 0.36, 1] }}
        >
          {type}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

function SubstitutionGlow({ tint }: { tint: string }) {
  return (
    <div className="relative grid place-items-center py-0.5">
      <div className="flex items-center gap-2">
        <motion.span
          animate={{ width: ['0%', '100%', '0%'] }}
          className="h-[2px] rounded-full"
          style={{
            background: `linear-gradient(90deg, transparent, ${tint}, transparent)`,
          }}
          transition={{
            duration: 2.2,
            ease: 'easeInOut',
            repeat: Number.POSITIVE_INFINITY,
          }}
        />
      </div>
      <span
        className="mt-1 font-mono text-[9px] font-semibold uppercase tracking-[0.18em]"
        style={{ color: tint }}
      >
        substitute &lt;T&gt;
      </span>
    </div>
  );
}
