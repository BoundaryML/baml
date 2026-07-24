'use client';

import { motion, useMotionValue, useSpring, useTransform } from 'framer-motion';
import {
  Braces,
  Bug,
  CheckCheck,
  GitBranch,
  type LucideIcon,
  RadioTower,
  Terminal,
} from 'lucide-react';
import Image from 'next/image';
import { useRef, useState } from 'react';
import { FeatureAnimation } from './feature-animations';

type Feature = {
  body: string;
  details: string[];
  Icon: LucideIcon;
  id: string;
  outcome: string;
  tint: string;
  title: string;
};

const FEATURES: Feature[] = [
  {
    body: 'Turn messy model responses into the shape your app declared, with structured failures when the output cannot be recovered.',
    details: ['schema-aware parser', 'repair passes', 'typed failure paths'],
    Icon: CheckCheck,
    id: 'parser',
    outcome: 'Less brittle JSON handling',
    tint: '#0F766E',
    title: 'Parser',
  },
  {
    body: 'Model prompts can share abstractions like normal code, so larger systems do not collapse into copy-pasted prompt files.',
    details: ['generic helpers', 'inline lambdas', 'namespaces'],
    Icon: Braces,
    id: 'generics',
    outcome: 'Reusable prompt architecture',
    tint: '#6D28D9',
    title: 'Generics',
  },
  {
    body: 'Give agents and humans a compiler-produced map of the project: functions, types, clients, tests, and call surfaces.',
    details: ['project summaries', 'agent context', 'compiler-backed facts'],
    Icon: Terminal,
    id: 'describe',
    outcome: 'Better context on demand',
    tint: '#2563EB',
    title: 'Describe',
  },
  {
    body: 'Stream typed partial objects instead of raw tokens, so interfaces and agents can react before the final result lands.',
    details: ['partial types', 'incremental UI', 'typed stream states'],
    Icon: RadioTower,
    id: 'streaming',
    outcome: 'Progress before completion',
    tint: '#B45309',
    title: 'Streaming',
  },
  {
    body: 'Keep prompt tests beside the prompt functions they exercise, with assertions that survive refactors better than screenshots.',
    details: ['inline testsets', 'typed assertions', 'local eval loops'],
    Icon: Bug,
    id: 'tests',
    outcome: 'Prompt changes with guardrails',
    tint: '#BE123C',
    title: 'Tests',
  },
  {
    body: 'Represent failures and tool choices as types, then handle retries and dispatch with exhaustive match branches.',
    details: ['typed errors', 'retry policy', 'union match dispatch'],
    Icon: GitBranch,
    id: 'typed-errors',
    outcome: 'Agent control flow you can audit',
    tint: '#4D7C0F',
    title: 'Typed Errors',
  },
];

export function WhyALanguage() {
  const [activeFeatureId, setActiveFeatureId] = useState(FEATURES[0].id);
  const activeFeature =
    FEATURES.find((feature) => feature.id === activeFeatureId) ?? FEATURES[0];

  return (
    <section aria-labelledby="features-heading" className="w-full bg-[#FBF7ED]">
      <div className="mx-auto max-w-[1600px] border-b border-[#D9D3C4] px-6 py-20 sm:px-12 sm:py-28">
        <div className="mx-auto grid max-w-[1240px] items-start gap-14 lg:grid-cols-[0.72fr_1.28fr] lg:gap-16">
          <div>
            <p className="mb-5 text-[13px] font-semibold uppercase tracking-[0.14em] text-[#8A8580]">
              Features
            </p>

            <h2
              className="mb-7 max-w-[560px] text-[clamp(2.15rem,4.4vw,4.4rem)] font-semibold leading-[0.98] tracking-[-0.03em] text-[#1A1612]"
              id="features-heading"
            >
              BAML is a programming language.
            </h2>

            <p className="max-w-[540px] text-[clamp(1rem,1.35vw,1.15rem)] leading-[1.62] text-[#5C5852]">
              It&apos;s built in{' '}
              <span className="font-semibold text-[#1A1612]">Rust</span> and
              used by some of the world&apos;s largest companies. It has a
              compiler, VM, LSP, formatter, type system with inferred error
              types, and drops into any system so teams can adopt it
              incrementally without rewriting their stack.
            </p>
          </div>

          <div className="grid items-stretch gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] lg:gap-6">
            <LanguagePanel
              activeFeatureId={activeFeatureId}
              activeTint={activeFeature.tint}
              onActiveFeatureChange={setActiveFeatureId}
            />
            <AnimationPanel feature={activeFeature} />
          </div>
        </div>
      </div>
    </section>
  );
}

function LanguagePanel({
  activeFeatureId,
  activeTint,
  onActiveFeatureChange,
}: {
  activeFeatureId: string;
  activeTint: string;
  onActiveFeatureChange: (id: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const mouseX = useMotionValue(0);
  const mouseY = useMotionValue(0);

  const rotateX = useSpring(useTransform(mouseY, [-0.5, 0.5], [-10, 12]), {
    damping: 22,
    mass: 0.6,
    stiffness: 180,
  });
  const rotateY = useSpring(useTransform(mouseX, [-0.5, 0.5], [-14, 14]), {
    damping: 22,
    mass: 0.6,
    stiffness: 180,
  });

  const handleMouseMove = (event: React.MouseEvent<HTMLDivElement>) => {
    const node = ref.current;
    if (!node) {
      return;
    }
    const rect = node.getBoundingClientRect();
    const px = (event.clientX - rect.left) / rect.width - 0.5;
    const py = (event.clientY - rect.top) / rect.height - 0.5;
    mouseX.set(px);
    mouseY.set(py);
  };

  const handleMouseLeave = () => {
    mouseX.set(0);
    mouseY.set(0);
  };

  return (
    <div
      onMouseLeave={handleMouseLeave}
      onMouseMove={handleMouseMove}
      ref={ref}
      style={{ perspective: '1600px' }}
    >
      <motion.div
        className="h-full"
        style={{
          rotateX,
          rotateY,
          transformOrigin: '50% 70%',
          transformStyle: 'preserve-3d',
        }}
      >
        <motion.div
          animate={{ y: [0, -4, 0] }}
          className="h-full"
          style={{ transformStyle: 'preserve-3d' }}
          transition={{
            duration: 5.6,
            ease: 'easeInOut',
            repeat: Number.POSITIVE_INFINITY,
          }}
        >
          <motion.div
            animate={{
              borderColor: `${activeTint}55`,
              boxShadow: `0 0 0 1px ${activeTint}22, 0 36px 60px -28px rgba(26,22,18,0.4), 0 14px 28px -14px rgba(26,22,18,0.2), inset 0 2px 0 rgba(255,255,255,0.85)`,
            }}
            className="flex h-full flex-col rounded-xl border bg-gradient-to-b from-[#FFFEF9] to-[#F4EDDC] p-5 sm:p-6"
            transition={{ duration: 0.3, ease: [0.22, 0.61, 0.36, 1] }}
          >
            <div className="mb-5 flex items-center justify-between gap-3">
              <div>
                <p className="mb-0.5 font-mono text-[10px] uppercase tracking-[0.18em] text-[#8A8580]">
                  Language Layer
                </p>
                <h3 className="text-lg font-semibold tracking-[-0.02em] text-[#1A1612]">
                  BAML Language Layer
                </h3>
              </div>
              <motion.div
                animate={{
                  boxShadow: [
                    '0 0 0 1px rgba(167,99,255,0.22), 0 0 14px rgba(167,99,255,0.16)',
                    '0 0 0 1px rgba(167,99,255,0.34), 0 0 22px rgba(167,99,255,0.24)',
                    '0 0 0 1px rgba(167,99,255,0.22), 0 0 14px rgba(167,99,255,0.16)',
                  ],
                }}
                className="grid size-10 shrink-0 place-items-center rounded-md border border-[#A763FF]/30 bg-white/80"
                transition={{
                  duration: 2.8,
                  ease: 'easeInOut',
                  repeat: Number.POSITIVE_INFINITY,
                }}
              >
                <Image
                  alt=""
                  aria-hidden
                  className="h-6 w-6 object-contain"
                  height={24}
                  src="/bamllogopurple.svg"
                  width={24}
                />
              </motion.div>
            </div>

            <div className="grid flex-1 grid-cols-2 gap-2 sm:grid-cols-2">
              {FEATURES.map((feature) => (
                <FeatureChip
                  feature={feature}
                  isActive={feature.id === activeFeatureId}
                  key={feature.id}
                  onActivate={() => onActiveFeatureChange(feature.id)}
                />
              ))}
            </div>
          </motion.div>
        </motion.div>
      </motion.div>
    </div>
  );
}

function AnimationPanel({ feature }: { feature: Feature }) {
  const Icon = feature.Icon;

  return (
    <div className="flex h-full flex-col gap-3 rounded-xl border border-[#D9D3C4] bg-[#FFFCF6]/90 p-5 shadow-[0_22px_70px_-46px_rgba(26,22,18,0.55)] backdrop-blur-md">
      <motion.div
        animate={{ opacity: 1, x: 0 }}
        className="flex items-start gap-3"
        initial={{ opacity: 0, x: 6 }}
        key={`caption-${feature.id}`}
        transition={{ duration: 0.28, ease: [0.22, 0.61, 0.36, 1] }}
      >
        <div
          className="grid size-9 shrink-0 place-items-center rounded-md border"
          style={{
            background: `${feature.tint}12`,
            borderColor: `${feature.tint}38`,
            color: feature.tint,
          }}
        >
          <Icon aria-hidden size={17} strokeWidth={1.9} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="font-mono text-[10px] uppercase tracking-[0.16em] text-[#8A8580]">
            {feature.outcome}
          </div>
          <h3 className="text-[16px] font-semibold leading-tight tracking-[-0.015em] text-[#1A1612]">
            {feature.title}
          </h3>
        </div>
      </motion.div>

      <div className="flex-1">
        <FeatureAnimation featureId={feature.id} tint={feature.tint} />
      </div>
    </div>
  );
}

function FeatureChip({
  feature,
  isActive,
  onActivate,
}: {
  feature: Feature;
  isActive: boolean;
  onActivate: () => void;
}) {
  const Icon = feature.Icon;

  return (
    <button
      aria-label={feature.title}
      aria-pressed={isActive}
      className="group flex flex-col items-start justify-center gap-2 overflow-hidden rounded-md border bg-white px-3 py-2.5 text-left shadow-[0_8px_24px_-22px_rgba(26,22,18,0.45)] transition-[background-color,border-color,box-shadow] hover:bg-white hover:shadow-[0_16px_34px_-26px_rgba(26,22,18,0.55)] focus:outline-none focus:ring-2 focus:ring-[#6D28D9]/25"
      onClick={onActivate}
      onFocus={onActivate}
      onMouseEnter={onActivate}
      style={{
        borderColor: isActive ? `${feature.tint}80` : '#CFC6B5',
        boxShadow: isActive
          ? `0 0 0 1px ${feature.tint}30, 0 16px 38px -28px ${feature.tint}`
          : undefined,
      }}
      type="button"
    >
      <span
        className="grid size-7 shrink-0 place-items-center rounded-md border"
        style={{
          background: `${feature.tint}12`,
          borderColor: `${feature.tint}33`,
          color: feature.tint,
        }}
      >
        <Icon aria-hidden size={14} strokeWidth={1.9} />
      </span>
      <span className="min-w-0">
        <span className="block text-[12.5px] font-semibold leading-tight text-[#1A1612]">
          {feature.title}
        </span>
        <span
          className="mt-0.5 block overflow-hidden text-[10.5px] leading-snug text-[#6B6258]"
          style={{
            display: '-webkit-box',
            WebkitBoxOrient: 'vertical',
            WebkitLineClamp: 2,
          }}
        >
          {feature.outcome}
        </span>
      </span>
    </button>
  );
}
