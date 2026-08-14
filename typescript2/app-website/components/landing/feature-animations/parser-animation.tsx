'use client';

import { AnimatePresence, motion } from 'framer-motion';
import { Check, Sparkles } from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';

const KEY_COLOR = '#82AAFF';
const STR_COLOR = '#7FD3C4';
const NUM_COLOR = '#E8B86D';
const PLAIN_COLOR = '#E8DFCF';
const STRIP_COLOR = '#E27B7B';

const STAGE_DURATION = 1300;
const STAGE_COUNT = 5;
// One extra step at the end to hold the final clean state before remounting.
const TICKS_PER_CYCLE = STAGE_COUNT + 1;

const STAGE_LABEL: { color: string; text: string }[] = [
  { color: STRIP_COLOR, text: 'malformed input' },
  { color: '#E8B86D', text: 'stripped fences + comments' },
  { color: '#E8B86D', text: 'normalized quotes' },
  { color: '#E8B86D', text: 'parsed numbers · trailing commas' },
  { color: '#7FD3C4', text: 'matches Receipt' },
];

export function ParserAnimation({ tint }: { tint: string }) {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setTick((t) => t + 1);
    }, STAGE_DURATION);
    return () => clearInterval(interval);
  }, []);

  const stage = Math.min(tick % TICKS_PER_CYCLE, STAGE_COUNT - 1);
  const cycle = Math.floor(tick / TICKS_PER_CYCLE);

  const isFinal = stage === STAGE_COUNT - 1;
  const headerColor = isFinal ? tint : stage === 0 ? STRIP_COLOR : '#E8B86D';
  const headerLabel = isFinal
    ? 'Receipt'
    : stage === 0
      ? 'model output'
      : 'repairing…';

  return (
    <div
      className="relative flex h-full flex-col overflow-hidden rounded-md border bg-[#1A1612] shadow-[0_18px_40px_-22px_rgba(26,22,18,0.55)]"
      style={{ borderColor: `${headerColor}66` }}
    >
      <div className="flex shrink-0 items-center justify-between border-b border-white/10 bg-white/[0.03] px-3 py-1.5">
        <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-[#8A8580]">
          receipt.json
        </span>
        <span
          className="flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.16em]"
          style={{ color: headerColor }}
        >
          <motion.span
            animate={{
              opacity: isFinal ? 1 : [0.4, 1, 0.4],
            }}
            className="size-1.5 rounded-full"
            style={{ backgroundColor: headerColor }}
            transition={{
              duration: 1.4,
              ease: 'easeInOut',
              repeat: isFinal ? 0 : Number.POSITIVE_INFINITY,
            }}
          />
          {headerLabel}
        </span>
      </div>

      <div
        className="min-h-0 flex-1 overflow-hidden p-3 font-mono text-[11px] leading-[1.55]"
        key={cycle}
        style={{ color: PLAIN_COLOR }}
      >
        <ReceiptDoc stage={stage} />
      </div>

      <FixFooter stage={stage} tint={tint} />
    </div>
  );
}

function ReceiptDoc({ stage }: { stage: number }) {
  return (
    <div>
      <CollapseLine atStage={1} stage={stage}>
        ```json
      </CollapseLine>
      <Line>{'{'}</Line>
      <Line>
        {'  '}
        <FadeIn atStage={2} color={KEY_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Tinted color={KEY_COLOR}>vendor</Tinted>
        <FadeIn atStage={2} color={KEY_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Plain>{': '}</Plain>
        <FadeOut atStage={2} stage={stage}>
          &apos;
        </FadeOut>
        <FadeIn atStage={2} color={STR_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Tinted color={STR_COLOR}>Whole Foods</Tinted>
        <FadeOut atStage={2} stage={stage}>
          &apos;
        </FadeOut>
        <FadeIn atStage={2} color={STR_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Plain>,</Plain>
      </Line>
      <Line>
        {'  '}
        <FadeIn atStage={2} color={KEY_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Tinted color={KEY_COLOR}>total</Tinted>
        <FadeIn atStage={2} color={KEY_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Plain>{': '}</Plain>
        <FadeOut atStage={3} stage={stage}>
          $
        </FadeOut>
        <Tinted color={NUM_COLOR}>48.20</Tinted>
        <Plain>,</Plain>
      </Line>
      <Line>
        {'  '}
        <FadeIn atStage={2} color={KEY_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Tinted color={KEY_COLOR}>items</Tinted>
        <FadeIn atStage={2} color={KEY_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Plain>{': ['}</Plain>
        <FadeOut atStage={2} stage={stage}>
          &apos;
        </FadeOut>
        <FadeIn atStage={2} color={STR_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Tinted color={STR_COLOR}>milk</Tinted>
        <FadeOut atStage={2} stage={stage}>
          &apos;
        </FadeOut>
        <FadeIn atStage={2} color={STR_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Plain>{', '}</Plain>
        <FadeOut atStage={2} stage={stage}>
          &apos;
        </FadeOut>
        <FadeIn atStage={2} color={STR_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <Tinted color={STR_COLOR}>bread</Tinted>
        <FadeOut atStage={2} stage={stage}>
          &apos;
        </FadeOut>
        <FadeIn atStage={2} color={STR_COLOR} stage={stage}>
          &quot;
        </FadeIn>
        <FadeOut atStage={3} stage={stage}>
          ,
        </FadeOut>
        <Plain>]</Plain>
      </Line>
      <CollapseLine atStage={1} stage={stage}>
        {'  // pulled from photo'}
      </CollapseLine>
      <Line>{'}'}</Line>
      <CollapseLine atStage={1} stage={stage}>
        ```
      </CollapseLine>
    </div>
  );
}

function Line({ children }: { children: ReactNode }) {
  return <div className="whitespace-pre">{children}</div>;
}

function Plain({ children }: { children: ReactNode }) {
  return <span style={{ color: PLAIN_COLOR }}>{children}</span>;
}

function Tinted({ children, color }: { children: ReactNode; color: string }) {
  return <span style={{ color }}>{children}</span>;
}

function FadeOut({
  atStage,
  children,
  stage,
}: {
  atStage: number;
  children: ReactNode;
  stage: number;
}) {
  const visible = stage < atStage;
  return (
    <motion.span
      animate={{
        maxWidth: visible ? '40px' : 0,
        opacity: visible ? 1 : 0,
      }}
      className="inline-block overflow-hidden whitespace-pre underline decoration-[#E27B7B]/70 decoration-wavy underline-offset-2"
      initial={false}
      style={{ color: STRIP_COLOR }}
      transition={{ duration: 0.45, ease: [0.22, 0.61, 0.36, 1] }}
    >
      {children}
    </motion.span>
  );
}

function FadeIn({
  atStage,
  children,
  color,
  stage,
}: {
  atStage: number;
  children: ReactNode;
  color: string;
  stage: number;
}) {
  const visible = stage >= atStage;
  return (
    <motion.span
      animate={{
        maxWidth: visible ? '40px' : 0,
        opacity: visible ? 1 : 0,
      }}
      className="inline-block overflow-hidden whitespace-pre"
      initial={false}
      style={{ color }}
      transition={{ duration: 0.45, ease: [0.22, 0.61, 0.36, 1] }}
    >
      {children}
    </motion.span>
  );
}

function CollapseLine({
  atStage,
  children,
  stage,
}: {
  atStage: number;
  children: ReactNode;
  stage: number;
}) {
  const visible = stage < atStage;
  return (
    <motion.div
      animate={{
        maxHeight: visible ? '32px' : 0,
        opacity: visible ? 1 : 0,
      }}
      className="overflow-hidden whitespace-pre underline decoration-[#E27B7B]/60 decoration-wavy underline-offset-2"
      initial={false}
      style={{ color: STRIP_COLOR }}
      transition={{ duration: 0.45, ease: [0.22, 0.61, 0.36, 1] }}
    >
      {children}
    </motion.div>
  );
}

function FixFooter({ stage, tint }: { stage: number; tint: string }) {
  const isFinal = stage === STAGE_COUNT - 1;
  const label = STAGE_LABEL[stage] ?? STAGE_LABEL[0];
  const color = isFinal ? tint : label.color;

  return (
    <div className="shrink-0 border-t border-white/10 bg-white/[0.03] px-3 py-1.5">
      <AnimatePresence mode="wait">
        <motion.div
          animate={{ opacity: 1, x: 0 }}
          className="flex items-center gap-2"
          exit={{ opacity: 0, x: 6 }}
          initial={{ opacity: 0, x: -6 }}
          key={stage}
          transition={{ duration: 0.28, ease: [0.22, 0.61, 0.36, 1] }}
        >
          <span
            className="grid size-4 place-items-center rounded-full"
            style={{
              backgroundColor: `${color}22`,
              color,
            }}
          >
            {stage === 0 ? (
              <Sparkles aria-hidden size={9} strokeWidth={2.4} />
            ) : (
              <Check aria-hidden size={9} strokeWidth={2.6} />
            )}
          </span>
          <span
            className="font-mono text-[10px] uppercase tracking-[0.14em]"
            style={{ color }}
          >
            {label.text}
          </span>
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
