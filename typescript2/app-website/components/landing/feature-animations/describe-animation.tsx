'use client';

import { motion } from 'framer-motion';
import { Sparkles, Terminal as TerminalIcon, Zap } from 'lucide-react';
import type { JSX } from 'react';
import { useEffect, useState } from 'react';

export function DescribeAnimation({ tint }: { tint: string }) {
  const [cycle, setCycle] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setCycle((c) => c + 1);
    }, 9000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div
      className="flex h-full flex-col overflow-hidden rounded-md border bg-[#0F0D0A] shadow-[0_18px_40px_-22px_rgba(26,22,18,0.6)]"
      style={{ borderColor: `${tint}55` }}
    >
      <div className="flex shrink-0 items-center justify-between border-b border-white/10 bg-white/[0.04] px-3 py-1.5">
        <div className="flex items-center gap-1.5">
          <TerminalIcon
            aria-hidden
            className="text-[#9B8E80]"
            size={12}
            strokeWidth={2}
          />
          <span className="font-mono text-[9.5px] uppercase tracking-[0.16em] text-[#8A8580]">
            ~/baml-project
          </span>
        </div>
        <span className="flex gap-1">
          <span className="size-2 rounded-full bg-[#3A332B]" />
          <span className="size-2 rounded-full bg-[#3A332B]" />
          <span
            className="size-2 rounded-full"
            style={{ backgroundColor: `${tint}88` }}
          />
        </span>
      </div>

      <div
        className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-hidden p-3 font-mono text-[10.5px] leading-[1.5]"
        key={cycle}
      >
        {/* Prompt */}
        <motion.div
          animate={{ opacity: 1, y: 0 }}
          className="whitespace-pre"
          initial={{ opacity: 0, y: 4 }}
          transition={{ duration: 0.25 }}
        >
          <span className="text-[#7FD3C4]">~/baml-project</span>{' '}
          <span className="text-[#9B8E80]">$</span>{' '}
          <span className="text-[#E8DFCF]">claude</span>
        </motion.div>

        {/* Human task */}
        <motion.div
          animate={{ opacity: 1, y: 0 }}
          className="whitespace-pre-wrap break-words"
          initial={{ opacity: 0, y: 4 }}
          transition={{
            delay: 0.6,
            duration: 0.28,
            ease: [0.22, 0.61, 0.36, 1],
          }}
        >
          <span className="mr-1 text-[#9B8E80]">&gt;</span>
          <span className="text-[#E8DFCF]">
            build a triage function that uses ExtractReceipt
          </span>
        </motion.div>

        {/* Claude thinking line */}
        <motion.div
          animate={{ opacity: 1, x: 0 }}
          className="flex items-baseline gap-1.5"
          initial={{ opacity: 0, x: -6 }}
          transition={{
            delay: 1.6,
            duration: 0.3,
            ease: [0.22, 0.61, 0.36, 1],
          }}
        >
          <span
            className="grid size-4 shrink-0 place-items-center rounded-full"
            style={{ backgroundColor: `${tint}22`, color: tint }}
          >
            <Sparkles aria-hidden size={9} strokeWidth={2.2} />
          </span>
          <span className="italic text-[#8A8580]">thinking…</span>
        </motion.div>

        {/* Thought bubble */}
        <motion.div
          animate={{ opacity: 1, x: 0 }}
          className="ml-5 rounded-sm border-l-2 pl-2 italic text-[#9B8E80]"
          initial={{ opacity: 0, x: -4 }}
          style={{ borderLeftColor: `${tint}66` }}
          transition={{
            delay: 2.3,
            duration: 0.32,
            ease: [0.22, 0.61, 0.36, 1],
          }}
        >
          I need to know{' '}
          <span className="text-[#7FD3C4] not-italic">ExtractReceipt</span>
          &apos;s signature and what types are
          <br />
          available before I can wire this through.
        </motion.div>

        {/* Tool call */}
        <motion.div
          animate={{ opacity: 1, y: 0 }}
          className="flex items-center"
          initial={{ opacity: 0, y: 4 }}
          transition={{
            delay: 3.4,
            duration: 0.28,
            ease: [0.22, 0.61, 0.36, 1],
          }}
        >
          <span
            className="mr-1.5 inline-flex items-center gap-1 rounded-sm px-1 py-0.5 text-[9px] font-semibold uppercase tracking-[0.12em]"
            style={{
              backgroundColor: `${tint}22`,
              color: tint,
            }}
          >
            <Zap aria-hidden size={9} strokeWidth={2.4} />
            tool
          </span>
          <span className="text-[#9B8E80]">$&nbsp;</span>
          <span className="text-[#E8DFCF]">baml describe&nbsp;</span>
          <span className="text-[#7FD3C4]">ExtractReceipt</span>
        </motion.div>

        {/* Describe output */}
        <DescribeOutput delay={4.0} tint={tint} />

        {/* Claude final */}
        <motion.div
          animate={{ opacity: 1, x: 0 }}
          className="flex items-baseline gap-1.5"
          initial={{ opacity: 0, x: -6 }}
          transition={{
            delay: 6.4,
            duration: 0.3,
            ease: [0.22, 0.61, 0.36, 1],
          }}
        >
          <span
            className="grid size-4 shrink-0 place-items-center rounded-full"
            style={{ backgroundColor: `${tint}22`, color: tint }}
          >
            <Sparkles aria-hidden size={9} strokeWidth={2.2} />
          </span>
          <span className="min-w-0 flex-1">
            <span className="text-[#E8DFCF]">Wiring through </span>
            <span className="text-[#7FD3C4]">ExtractReceipt</span>
            <span className="text-[#E8DFCF]"> → </span>
            <span className="text-[#82AAFF]">Triage</span>
            <span className="text-[#9B8E80]">{' { priority, category }'}</span>
          </span>
        </motion.div>
      </div>
    </div>
  );
}

const DESCRIBE_LINES: { color: string; key: string; text: string }[] = [
  { key: 'sep', color: '#3A332B', text: '── ExtractReceipt ──' },
  { key: 'def', color: '#E8DFCF', text: 'defined: receipt.baml:14' },
  { key: 'args', color: '#E8DFCF', text: 'args:    text: string' },
  {
    key: 'returns',
    color: '#E8DFCF',
    text: 'returns: Receipt { vendor, total, items[] }',
  },
  { key: 'tests', color: '#E8DFCF', text: 'tests:   3 passing' },
];

function DescribeOutput({ delay, tint }: { delay: number; tint: string }) {
  return (
    <motion.div
      animate={{ opacity: 1 }}
      className="ml-3 rounded-sm border border-[#3A332B] bg-white/[0.02] px-2 py-1"
      initial={{ opacity: 0 }}
      transition={{ delay, duration: 0.2 }}
    >
      {DESCRIBE_LINES.map((line, i) => (
        <motion.div
          animate={{ opacity: 1, x: 0 }}
          className="whitespace-pre text-[10px]"
          initial={{ opacity: 0, x: -4 }}
          key={line.key}
          style={{ color: line.color }}
          transition={{
            delay: delay + 0.2 + i * 0.22,
            duration: 0.25,
            ease: [0.22, 0.61, 0.36, 1],
          }}
        >
          {colorizeKeyValue(line.text, line.key, tint)}
        </motion.div>
      ))}
    </motion.div>
  );
}

function colorizeKeyValue(
  text: string,
  key: string,
  tint: string,
): JSX.Element {
  if (key === 'sep') {
    return <span style={{ color: '#3A332B' }}>{text}</span>;
  }

  const colonIdx = text.indexOf(':');
  if (colonIdx < 0) {
    return <span>{text}</span>;
  }

  const label = text.slice(0, colonIdx);
  const valuePart = text.slice(colonIdx + 1).trimStart();
  const padding = ' '.repeat(
    text.slice(colonIdx + 1).length - valuePart.length,
  );

  return (
    <>
      <span style={{ color: tint }}>{label}</span>
      <span style={{ color: '#9B8E80' }}>:{padding}</span>
      <span>{renderValue(key, valuePart)}</span>
    </>
  );
}

function renderValue(key: string, value: string): JSX.Element {
  if (key === 'def') {
    return <span style={{ color: '#7FD3C4' }}>{value}</span>;
  }
  if (key === 'args') {
    return (
      <>
        <span style={{ color: '#E8DFCF' }}>text: </span>
        <span style={{ color: '#82AAFF' }}>string</span>
      </>
    );
  }
  if (key === 'returns') {
    return (
      <>
        <span style={{ color: '#82AAFF' }}>Receipt</span>
        <span style={{ color: '#9B8E80' }}>{' { '}</span>
        <span style={{ color: '#E8DFCF' }}>vendor, total, items[]</span>
        <span style={{ color: '#9B8E80' }}>{' }'}</span>
      </>
    );
  }
  if (key === 'tests') {
    return (
      <>
        <span style={{ color: '#7FD3C4' }}>3</span>
        <span style={{ color: '#E8DFCF' }}> passing</span>
      </>
    );
  }
  return <span>{value}</span>;
}
