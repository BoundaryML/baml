'use client';

import { motion } from 'framer-motion';
import {
  AlertTriangle,
  ArrowDown,
  Check,
  Clock,
  RefreshCw,
} from 'lucide-react';
import { useEffect, useState } from 'react';

type Outcome = 'ok' | 'parse-error' | 'timeout-error';

const SCENARIOS: {
  catchArm: 'parse' | 'retry' | null;
  color: string;
  input: string;
  outcome: Outcome;
  outputLabel: string;
}[] = [
  {
    input: '"introduction"',
    outcome: 'ok',
    outputLabel: '"introduction"',
    catchArm: null,
    color: '#7FD3C4',
  },
  {
    input: '"   "',
    outcome: 'parse-error',
    outputLabel: '"untitled"',
    catchArm: 'parse',
    color: '#E27B7B',
  },
  {
    input: '<remote>',
    outcome: 'timeout-error',
    outputLabel: 'retry → "ok"',
    catchArm: 'retry',
    color: '#E8B86D',
  },
];

export function TypedErrorsAnimation({ tint }: { tint: string }) {
  const [idx, setIdx] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setIdx((i) => (i + 1) % SCENARIOS.length);
    }, 2800);
    return () => clearInterval(interval);
  }, []);

  const scenario = SCENARIOS[idx];

  return (
    <div className="flex h-full flex-col gap-2.5">
      <div
        className="rounded-md border bg-[#1A1612] p-3 font-mono text-[11px] leading-[1.55] shadow-[0_12px_30px_-22px_rgba(26,22,18,0.55)]"
        style={{ borderColor: `${tint}55` }}
      >
        <div className="mb-1 flex items-center justify-between">
          <span className="font-mono text-[9px] uppercase tracking-[0.16em] text-[#8A8580]">
            errors.baml
          </span>
          <span
            className="font-mono text-[9px] uppercase tracking-[0.16em]"
            style={{ color: tint }}
          >
            throws
          </span>
        </div>
        <div className="text-[#E8DFCF]">
          <span className="text-[#7FD3C4]">RequireTitle</span>(
          <motion.span
            animate={{ opacity: 1 }}
            initial={{ opacity: 0 }}
            key={`call-${idx}`}
            transition={{ duration: 0.2 }}
          >
            <span className="text-[#E8B86D]">{scenario.input}</span>
          </motion.span>
          )
        </div>
      </div>

      <BranchFork active={scenario.outcome} tint={tint} />

      <div
        className="flex-1 rounded-md border bg-[#1A1612] p-3 font-mono text-[11px] leading-[1.55] shadow-[0_12px_30px_-22px_rgba(26,22,18,0.55)]"
        style={{ borderColor: `${tint}55` }}
      >
        <div className="mb-1 flex items-center justify-between">
          <span className="font-mono text-[9px] uppercase tracking-[0.16em] text-[#8A8580]">
            union match
          </span>
          {scenario.catchArm === 'retry' ? (
            <RetryChip tint={scenario.color} />
          ) : null}
        </div>
        <div className="text-[#E8DFCF]">
          <span className="text-[#C792EA]">catch</span> (e) &#123;
        </div>
        <div className="ml-3 mt-1 flex flex-col gap-1.5">
          <CatchArm
            active={scenario.catchArm === 'parse'}
            errorType="ParseError"
            handler={`=> "untitled"`}
            tint={SCENARIOS[1].color}
          />
          <CatchArm
            active={scenario.catchArm === 'retry'}
            errorType="TimeoutError"
            handler={`=> retry()`}
            tint={SCENARIOS[2].color}
          />
        </div>
        <div className="mt-1 text-[#E8DFCF]">&#125;</div>

        <ResultRow
          color={scenario.color}
          outcome={scenario.outcome}
          value={scenario.outputLabel}
        />
      </div>
    </div>
  );
}

function BranchFork({ active, tint }: { active: Outcome; tint: string }) {
  const branches: { color: string; key: Outcome; label: string }[] = [
    { key: 'ok', label: 'ok', color: '#7FD3C4' },
    { key: 'parse-error', label: 'ParseError', color: '#E27B7B' },
    { key: 'timeout-error', label: 'TimeoutError', color: '#E8B86D' },
  ];

  return (
    <div className="grid grid-cols-3 gap-2">
      {branches.map((b) => {
        const isActive = active === b.key;
        return (
          <motion.div
            animate={{
              backgroundColor: isActive ? `${b.color}1f` : 'rgba(0,0,0,0)',
              borderColor: isActive ? `${b.color}80` : '#3A332B',
              opacity: isActive ? 1 : 0.45,
            }}
            className="flex items-center justify-center gap-1.5 rounded-sm border px-2 py-1.5"
            key={b.key}
            transition={{ duration: 0.3, ease: [0.22, 0.61, 0.36, 1] }}
          >
            {b.key === 'ok' ? (
              <Check
                aria-hidden
                size={11}
                strokeWidth={2.4}
                style={{ color: b.color }}
              />
            ) : (
              <AlertTriangle
                aria-hidden
                size={11}
                strokeWidth={2.2}
                style={{ color: b.color }}
              />
            )}
            <span
              className="font-mono text-[10px] font-semibold"
              style={{ color: isActive ? b.color : '#9B8E80' }}
            >
              {b.label}
            </span>
            {isActive ? (
              <motion.span
                animate={{ y: [0, 4, 0] }}
                className="ml-0.5"
                style={{ color: b.color }}
                transition={{
                  duration: 1.2,
                  ease: 'easeInOut',
                  repeat: Number.POSITIVE_INFINITY,
                }}
              >
                <ArrowDown aria-hidden size={11} strokeWidth={2.2} />
              </motion.span>
            ) : null}
          </motion.div>
        );
      })}
    </div>
  );
}

function CatchArm({
  active,
  errorType,
  handler,
  tint,
}: {
  active: boolean;
  errorType: string;
  handler: string;
  tint: string;
}) {
  return (
    <motion.div
      animate={{
        backgroundColor: active ? `${tint}1f` : 'transparent',
        borderColor: active ? `${tint}66` : 'transparent',
      }}
      className="flex items-baseline gap-1.5 rounded-sm border px-1.5 py-0.5"
      transition={{ duration: 0.3 }}
    >
      <span className="text-[#9B8E80]">_:</span>
      <span
        className="font-semibold"
        style={{ color: active ? tint : '#7FD3C4' }}
      >
        {errorType}
      </span>
      <span className="text-[#E8DFCF]">{handler}</span>
    </motion.div>
  );
}

function RetryChip({ tint }: { tint: string }) {
  return (
    <motion.span
      animate={{ opacity: 1, scale: 1 }}
      className="flex items-center gap-1 rounded-sm border px-1.5 py-px"
      initial={{ opacity: 0, scale: 0.8 }}
      style={{
        backgroundColor: `${tint}1f`,
        borderColor: `${tint}66`,
        color: tint,
      }}
      transition={{ duration: 0.25 }}
    >
      <motion.span
        animate={{ rotate: 360 }}
        className="grid place-items-center"
        transition={{
          duration: 1.4,
          ease: 'linear',
          repeat: Number.POSITIVE_INFINITY,
        }}
      >
        <RefreshCw aria-hidden size={9} strokeWidth={2.4} />
      </motion.span>
      <span className="font-mono text-[9px] uppercase tracking-[0.16em]">
        retry · 1.5s
      </span>
    </motion.span>
  );
}

function ResultRow({
  color,
  outcome,
  value,
}: {
  color: string;
  outcome: Outcome;
  value: string;
}) {
  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className="mt-2 flex items-center gap-2 rounded-sm border px-2 py-1"
      initial={{ opacity: 0, y: 6 }}
      key={value}
      style={{
        backgroundColor: `${color}18`,
        borderColor: `${color}55`,
      }}
      transition={{ delay: 0.4, duration: 0.3 }}
    >
      {outcome === 'ok' ? (
        <Check aria-hidden size={11} strokeWidth={2.4} style={{ color }} />
      ) : (
        <Clock aria-hidden size={11} strokeWidth={2.2} style={{ color }} />
      )}
      <span className="font-mono text-[10.5px] font-semibold" style={{ color }}>
        {value}
      </span>
    </motion.div>
  );
}
