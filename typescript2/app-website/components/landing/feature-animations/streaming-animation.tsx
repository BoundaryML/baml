'use client';

import { motion } from 'framer-motion';
import { Asterisk, Lock, Radio } from 'lucide-react';
import { useEffect, useState } from 'react';

type FieldState = 'pending' | 'streaming' | 'done';

type Field = {
  annotation: 'done' | 'not_null' | 'with_state';
  fillTokens: string[];
  name: string;
  type: string;
};

const FIELDS: Field[] = [
  {
    annotation: 'done',
    fillTokens: ['"Whole', '"Whole Fo', '"Whole Foods"'],
    name: 'vendor',
    type: 'string',
  },
  {
    annotation: 'not_null',
    fillTokens: ['4', '48', '48.2', '48.20'],
    name: 'total',
    type: 'float',
  },
  {
    annotation: 'with_state',
    fillTokens: [
      '"Receipt',
      '"Receipt for',
      '"Receipt for groceries on',
      '"Receipt for groceries on Mar 14"',
    ],
    name: 'description',
    type: 'string',
  },
  {
    annotation: 'done',
    fillTokens: ['[', '[1 item]', '[2 items]', '[3 items]'],
    name: 'items',
    type: 'Item[]',
  },
];

type Tick = {
  field: number;
  fillIndex: number;
};

function buildSchedule(): Tick[] {
  const ticks: Tick[] = [];
  for (let f = 0; f < FIELDS.length; f++) {
    for (let t = 0; t < FIELDS[f].fillTokens.length; t++) {
      ticks.push({ field: f, fillIndex: t });
    }
  }
  return ticks;
}

const SCHEDULE = buildSchedule();

export function StreamingAnimation({ tint }: { tint: string }) {
  const [tickIdx, setTickIdx] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setTickIdx((i) => (i + 1) % (SCHEDULE.length + 4));
    }, 420);
    return () => clearInterval(interval);
  }, []);

  const fieldStates: { state: FieldState; value: string }[] = FIELDS.map(
    () => ({ state: 'pending', value: '' }),
  );

  for (let i = 0; i <= tickIdx && i < SCHEDULE.length; i++) {
    const t = SCHEDULE[i];
    const field = FIELDS[t.field];
    const isLastFill = t.fillIndex === field.fillTokens.length - 1;
    fieldStates[t.field] = {
      state: isLastFill ? 'done' : 'streaming',
      value: field.fillTokens[t.fillIndex],
    };
  }

  return (
    <div className="flex h-full flex-col gap-2.5">
      <StreamHeader active={tickIdx < SCHEDULE.length} tint={tint} />
      <div
        className="relative flex-1 overflow-hidden rounded-md border bg-[#1A1612] p-3 font-mono text-[11px] leading-[1.55] shadow-[0_12px_30px_-22px_rgba(26,22,18,0.55)]"
        style={{ borderColor: `${tint}55` }}
      >
        <div className="text-[#E8DFCF]">
          <span className="font-semibold text-[#C792EA]">class</span>{' '}
          <span className="text-[#7FD3C4]">Receipt</span> <span>&#123;</span>
        </div>
        <div className="ml-3 mt-1 flex flex-col gap-1.5">
          {FIELDS.map((field, i) => (
            <FieldRow
              annotation={field.annotation}
              key={field.name}
              name={field.name}
              state={fieldStates[i].state}
              tint={tint}
              type={field.type}
              value={fieldStates[i].value}
            />
          ))}
        </div>
        <div className="mt-1 text-[#E8DFCF]">&#125;</div>
      </div>
    </div>
  );
}

function StreamHeader({ active, tint }: { active: boolean; tint: string }) {
  return (
    <div className="flex items-center justify-between rounded-sm border border-[#3A332B] bg-[#1A1612] px-2.5 py-1">
      <div className="flex items-center gap-1.5">
        <motion.span
          animate={{ opacity: active ? [0.3, 1, 0.3] : 0.4 }}
          className="grid size-4 place-items-center rounded-full"
          style={{ backgroundColor: `${tint}22`, color: tint }}
          transition={{
            duration: 1,
            ease: 'easeInOut',
            repeat: Number.POSITIVE_INFINITY,
          }}
        >
          <Radio aria-hidden size={9} strokeWidth={2.2} />
        </motion.span>
        <span className="font-mono text-[9px] uppercase tracking-[0.16em] text-[#8A8580]">
          {active ? 'streaming tokens…' : 'stream complete'}
        </span>
      </div>
      <span
        className="font-mono text-[9px] uppercase tracking-[0.16em]"
        style={{ color: tint }}
      >
        b.stream.ExtractReceipt
      </span>
    </div>
  );
}

function FieldRow({
  annotation,
  name,
  state,
  tint,
  type,
  value,
}: {
  annotation: 'done' | 'not_null' | 'with_state';
  name: string;
  state: FieldState;
  tint: string;
  type: string;
  value: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <div className="flex w-[22px] shrink-0 items-center justify-center">
        <AnnotationIcon annotation={annotation} state={state} tint={tint} />
      </div>
      <span className="w-[78px] shrink-0 text-[#E8DFCF]">{name}</span>
      <span className="w-[58px] shrink-0 text-[#82AAFF]">{type}</span>
      <ValueCell state={state} tint={tint} value={value} />
    </div>
  );
}

function AnnotationIcon({
  annotation,
  state,
  tint,
}: {
  annotation: 'done' | 'not_null' | 'with_state';
  state: FieldState;
  tint: string;
}) {
  const active = state !== 'pending';
  const dim = state === 'pending';

  if (annotation === 'done') {
    return (
      <motion.div
        animate={{
          backgroundColor: state === 'done' ? `${tint}33` : 'transparent',
          color: state === 'done' ? tint : dim ? '#3A332B' : '#9B8E80',
        }}
        className="grid size-4 place-items-center rounded-sm border"
        style={{
          borderColor: state === 'done' ? `${tint}66` : '#3A332B',
        }}
        transition={{ duration: 0.25 }}
      >
        <Lock aria-hidden size={9} strokeWidth={2.2} />
      </motion.div>
    );
  }
  if (annotation === 'not_null') {
    return (
      <motion.div
        animate={{
          backgroundColor: active ? `${tint}33` : 'transparent',
          color: active ? tint : '#3A332B',
        }}
        className="grid size-4 place-items-center rounded-sm border"
        style={{
          borderColor: active ? `${tint}66` : '#3A332B',
        }}
        transition={{ duration: 0.25 }}
      >
        <Asterisk aria-hidden size={9} strokeWidth={2.6} />
      </motion.div>
    );
  }
  return (
    <div
      className="relative h-[3px] w-4 overflow-hidden rounded-full"
      style={{ backgroundColor: '#3A332B' }}
    >
      <motion.div
        animate={{
          width:
            state === 'done' ? '100%' : state === 'streaming' ? '60%' : '0%',
        }}
        className="absolute left-0 top-0 h-full rounded-full"
        style={{ backgroundColor: tint }}
        transition={{ duration: 0.35, ease: 'easeOut' }}
      />
    </div>
  );
}

function ValueCell({
  state,
  tint,
  value,
}: {
  state: FieldState;
  tint: string;
  value: string;
}) {
  if (state === 'pending') {
    return (
      <span className="flex items-center gap-1.5 text-[#3A332B]">
        <span className="inline-block h-[1px] w-3 bg-[#3A332B]" />
      </span>
    );
  }
  return (
    <motion.span
      animate={{ opacity: 1 }}
      className="flex min-w-0 items-center gap-1.5"
      initial={{ opacity: 0 }}
      key={value}
      transition={{ duration: 0.16 }}
    >
      <span
        className="truncate text-[#E8B86D]"
        style={{ color: state === 'done' ? '#E8B86D' : `${tint}` }}
      >
        {value}
      </span>
      {state === 'streaming' ? (
        <motion.span
          animate={{ opacity: [0, 1, 0] }}
          className="inline-block h-2.5 w-[2px]"
          style={{ backgroundColor: tint }}
          transition={{
            duration: 0.6,
            ease: 'easeInOut',
            repeat: Number.POSITIVE_INFINITY,
          }}
        />
      ) : null}
    </motion.span>
  );
}
