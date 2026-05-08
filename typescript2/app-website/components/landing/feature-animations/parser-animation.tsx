'use client';

import { AnimatePresence, motion } from 'framer-motion';
import { Check, Sparkles } from 'lucide-react';
import { useEffect, useState } from 'react';

type Stage = {
  badTokens: boolean;
  fix: string;
  status: 'malformed' | 'repairing' | 'parsed';
  text: string;
};

const STAGES: Stage[] = [
  {
    status: 'malformed',
    fix: 'malformed input',
    badTokens: true,
    text: `\`\`\`json
{
  vendor: 'Whole Foods',
  total: $48.20,
  items: ['milk', 'bread',],
  // pulled from photo
}
\`\`\``,
  },
  {
    status: 'repairing',
    fix: 'stripped fences + comments',
    badTokens: true,
    text: `{
  vendor: 'Whole Foods',
  total: $48.20,
  items: ['milk', 'bread',]
}`,
  },
  {
    status: 'repairing',
    fix: 'normalized quotes',
    badTokens: true,
    text: `{
  "vendor": "Whole Foods",
  "total": $48.20,
  "items": ["milk", "bread",]
}`,
  },
  {
    status: 'repairing',
    fix: 'parsed numbers · trailing commas',
    badTokens: false,
    text: `{
  "vendor": "Whole Foods",
  "total": 48.20,
  "items": ["milk", "bread"]
}`,
  },
  {
    status: 'parsed',
    fix: 'matches Receipt',
    badTokens: false,
    text: `{
  "vendor": "Whole Foods",
  "total": 48.20,
  "items": ["milk", "bread"]
}`,
  },
];

const STAGE_DURATIONS = [1900, 1800, 1800, 1800, 2400];

export function ParserAnimation({ tint }: { tint: string }) {
  const [stageIdx, setStageIdx] = useState(0);

  useEffect(() => {
    const timeout = setTimeout(() => {
      setStageIdx((i) => (i + 1) % STAGES.length);
    }, STAGE_DURATIONS[stageIdx]);
    return () => clearTimeout(timeout);
  }, [stageIdx]);

  const stage = STAGES[stageIdx];

  return (
    <div
      className="relative flex h-full flex-col overflow-hidden rounded-md border bg-[#1A1612] shadow-[0_18px_40px_-22px_rgba(26,22,18,0.55)]"
      style={{
        borderColor:
          stage.status === 'parsed' ? `${tint}99` : `${stage.status === 'malformed' ? '#E27B7B' : '#E8B86D'}66`,
      }}
    >
      <Header stage={stage} tint={tint} />

      <div className="relative min-h-0 flex-1">
        <AnimatePresence mode="wait">
          <motion.pre
            animate={{ opacity: 1, y: 0 }}
            className="absolute inset-0 overflow-auto p-3 font-mono text-[11px] leading-[1.55]"
            exit={{ opacity: 0, y: -6 }}
            initial={{ opacity: 0, y: 8 }}
            key={stageIdx}
            transition={{ duration: 0.32, ease: [0.22, 0.61, 0.36, 1] }}
          >
            <code>
              {stage.badTokens ? (
                <MessyHighlighted text={stage.text} />
              ) : (
                <CleanCode text={stage.text} />
              )}
            </code>
          </motion.pre>
        </AnimatePresence>
      </div>

      <FixFooter stage={stage} stageIdx={stageIdx} tint={tint} />
    </div>
  );
}

function Header({ stage, tint }: { stage: Stage; tint: string }) {
  const color =
    stage.status === 'malformed'
      ? '#E27B7B'
      : stage.status === 'parsed'
        ? tint
        : '#E8B86D';
  const label =
    stage.status === 'malformed'
      ? 'model output'
      : stage.status === 'parsed'
        ? 'Receipt'
        : 'repairing…';

  return (
    <div className="flex shrink-0 items-center justify-between border-b border-white/10 bg-white/[0.03] px-3 py-1.5">
      <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-[#8A8580]">
        receipt.json
      </span>
      <span
        className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.16em]"
        style={{ color }}
      >
        <motion.span
          animate={{
            opacity: stage.status === 'parsed' ? 1 : [0.4, 1, 0.4],
          }}
          className="size-1.5 rounded-full"
          style={{ backgroundColor: color }}
          transition={{
            duration: 1.4,
            ease: 'easeInOut',
            repeat:
              stage.status === 'parsed' ? 0 : Number.POSITIVE_INFINITY,
          }}
        />
        {label}
      </span>
    </div>
  );
}

const MESSY_ERROR_PATTERNS = [
  /'[^']*'/g,
  /\$\d+(?:\.\d+)?/g,
  /\/\/[^\n]*/g,
  /<thinking>[\s\S]*?<\/thinking>/g,
  /```(?:json)?/g,
];

function MessyHighlighted({ text }: { text: string }) {
  const errorRanges: { color: string; end: number; start: number }[] = [];
  for (const pattern of MESSY_ERROR_PATTERNS) {
    const re = new RegExp(pattern.source, pattern.flags);
    let match: RegExpExecArray | null;
    match = re.exec(text);
    while (match !== null) {
      errorRanges.push({
        color: '#E27B7B',
        end: match.index + match[0].length,
        start: match.index,
      });
      match = re.exec(text);
    }
  }
  errorRanges.sort((a, b) => a.start - b.start);

  const segments: { color: string; text: string }[] = [];
  let cursor = 0;
  for (const range of errorRanges) {
    if (range.start < cursor) {
      continue;
    }
    if (range.start > cursor) {
      segments.push({
        color: '#9B8E80',
        text: text.slice(cursor, range.start),
      });
    }
    segments.push({
      color: range.color,
      text: text.slice(range.start, range.end),
    });
    cursor = range.end;
  }
  if (cursor < text.length) {
    segments.push({ color: '#9B8E80', text: text.slice(cursor) });
  }

  return (
    <>
      {segments.map((segment, i) => (
        <span
          className={
            segment.color === '#E27B7B'
              ? 'underline decoration-[#E27B7B]/60 decoration-wavy underline-offset-2'
              : ''
          }
          key={`${i}-${segment.text.slice(0, 8)}`}
          style={{ color: segment.color }}
        >
          {segment.text}
        </span>
      ))}
    </>
  );
}

const CLEAN_KEYS = /"[^"]*"(?=\s*:)/g;
const CLEAN_STRINGS = /"[^"]*"/g;
const CLEAN_NUMBERS = /\b\d+(?:\.\d+)?\b/g;

function CleanCode({ text }: { text: string }) {
  const ranges: { color: string; end: number; start: number }[] = [];

  const collect = (re: RegExp, color: string) => {
    const fresh = new RegExp(re.source, re.flags);
    let match: RegExpExecArray | null;
    match = fresh.exec(text);
    while (match !== null) {
      ranges.push({
        color,
        end: match.index + match[0].length,
        start: match.index,
      });
      match = fresh.exec(text);
    }
  };

  collect(CLEAN_KEYS, '#82AAFF');
  collect(CLEAN_NUMBERS, '#E8B86D');

  const taken = new Set<number>();
  for (const r of ranges) {
    for (let i = r.start; i < r.end; i++) {
      taken.add(i);
    }
  }
  const stringRe = new RegExp(CLEAN_STRINGS.source, CLEAN_STRINGS.flags);
  let m: RegExpExecArray | null;
  m = stringRe.exec(text);
  while (m !== null) {
    if (!taken.has(m.index)) {
      ranges.push({
        color: '#7FD3C4',
        end: m.index + m[0].length,
        start: m.index,
      });
    }
    m = stringRe.exec(text);
  }

  ranges.sort((a, b) => a.start - b.start);

  const segments: { color: string; text: string }[] = [];
  let cursor = 0;
  for (const r of ranges) {
    if (r.start < cursor) {
      continue;
    }
    if (r.start > cursor) {
      segments.push({ color: '#E8DFCF', text: text.slice(cursor, r.start) });
    }
    segments.push({
      color: r.color,
      text: text.slice(r.start, r.end),
    });
    cursor = r.end;
  }
  if (cursor < text.length) {
    segments.push({ color: '#E8DFCF', text: text.slice(cursor) });
  }

  return (
    <>
      {segments.map((segment, i) => (
        <span
          key={`${i}-${segment.text.slice(0, 8)}`}
          style={{ color: segment.color }}
        >
          {segment.text}
        </span>
      ))}
    </>
  );
}

function FixFooter({
  stage,
  stageIdx,
  tint,
}: {
  stage: Stage;
  stageIdx: number;
  tint: string;
}) {
  const color =
    stage.status === 'malformed'
      ? '#E27B7B'
      : stage.status === 'parsed'
        ? tint
        : '#E8B86D';

  return (
    <div className="shrink-0 border-t border-white/10 bg-white/[0.03] px-3 py-1.5">
      <AnimatePresence mode="wait">
        <motion.div
          animate={{ opacity: 1, x: 0 }}
          className="flex items-center gap-2"
          exit={{ opacity: 0, x: 6 }}
          initial={{ opacity: 0, x: -6 }}
          key={stageIdx}
          transition={{ duration: 0.28, ease: [0.22, 0.61, 0.36, 1] }}
        >
          <span
            className="grid size-4 place-items-center rounded-full"
            style={{
              backgroundColor: `${color}22`,
              color,
            }}
          >
            {stage.status === 'malformed' ? (
              <Sparkles aria-hidden size={9} strokeWidth={2.4} />
            ) : (
              <Check aria-hidden size={9} strokeWidth={2.6} />
            )}
          </span>
          <span
            className="font-mono text-[10px] uppercase tracking-[0.14em]"
            style={{ color }}
          >
            {stage.fix}
          </span>
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
