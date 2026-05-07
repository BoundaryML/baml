'use client';

import { motion } from 'framer-motion';
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
import type { ReactNode } from 'react';
import { useState } from 'react';
import type { IconType } from 'react-icons';
import {
  SiClaude,
  SiGo,
  SiGooglegemini,
  SiOpenai,
  SiOpenjdk,
  SiRuby,
  SiTypescript,
} from 'react-icons/si';

type Feature = {
  body: string;
  code: string;
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
    code: `class Receipt {
  vendor string
  total float
}

function ExtractReceipt(text: string) -> Receipt {
  client GPT4o
  prompt #"
    Extract the receipt fields.
    {{ ctx.output_format }}

    {{ _.role("user") }} {{ text }}
  "#
}`,
    details: ['schema-aware parser', 'repair passes', 'typed failure paths'],
    Icon: CheckCheck,
    id: 'parser',
    outcome: 'Less brittle JSON handling',
    tint: '#0F766E',
    title: 'Parser',
  },
  {
    body: 'Model prompts can share abstractions like normal code, so larger systems do not collapse into copy-pasted prompt files.',
    code: `// baml_src/ns_eval/metrics.baml
class Score {
  passed bool
  reason string
}

function First<T>(items: T[]) -> T? {
  items.at(0)
}`,
    details: ['generic helpers', 'inline lambdas', 'namespaces'],
    Icon: Braces,
    id: 'generics',
    outcome: 'Reusable prompt architecture',
    tint: '#6D28D9',
    title: 'Generics',
  },
  {
    body: 'Give agents and humans a compiler-produced map of the project: functions, types, clients, tests, and call surfaces.',
    code: `// baml describe surfaces this shape
class Config {
  model string
}

function Judge(
  config: root.Config,
  output: string
) -> root.eval.Score {
  root.eval.Score {
    passed: output.length() > 0,
    reason: "checked with " + config.model,
  }
}`,
    details: ['project summaries', 'agent context', 'compiler-backed facts'],
    Icon: Terminal,
    id: 'describe',
    outcome: 'Better context on demand',
    tint: '#2563EB',
    title: 'Describe',
  },
  {
    body: 'Stream typed partial objects instead of raw tokens, so interfaces and agents can react before the final result lands.',
    code: `class Agenda {
  start_time string @stream.done
  items (Talk | Social)[]
  description string @stream.with_state
}

class Talk {
  type "talk" @stream.not_null
  title string
}`,
    details: ['partial types', 'incremental UI', 'typed stream states'],
    Icon: RadioTower,
    id: 'streaming',
    outcome: 'Progress before completion',
    tint: '#B45309',
    title: 'Streaming',
  },
  {
    body: 'Keep prompt tests beside the prompt functions they exercise, with assertions that survive refactors better than screenshots.',
    code: `testset "triage" {
  test "password reset" {
    let ticket = TriageTicket(
      "I cannot reset my password"
    );

    assert.equal(ticket.priority, "medium");
  }
}`,
    details: ['inline testsets', 'typed assertions', 'local eval loops'],
    Icon: Bug,
    id: 'tests',
    outcome: 'Prompt changes with guardrails',
    tint: '#BE123C',
    title: 'Tests',
  },
  {
    body: 'Represent failures and tool choices as types, then handle retries and dispatch with exhaustive match branches.',
    code: `class ParseError {
  message string
}

function RequireTitle(value: string) -> string throws ParseError {
  if (value.trim().length() == 0) {
    throw ParseError { message: "missing title" };
  };
  value
}

function SafeTitle(value: string) -> string {
  RequireTitle(value) catch (e) {
    _: ParseError => "untitled",
  }
}`,
    details: ['typed errors', 'retry policy', 'union match dispatch'],
    Icon: GitBranch,
    id: 'typed-errors',
    outcome: 'Agent control flow you can audit',
    tint: '#4D7C0F',
    title: 'Typed Errors',
  },
];

const APP_CLIENTS: {
  color: string;
  Icon?: IconType;
  imageSrc?: string;
  label: string;
  symbol?: string;
}[] = [
  { color: '#3776AB', imageSrc: '/python-icon.png', label: 'Python' },
  { color: '#3178C6', Icon: SiTypescript, label: 'TypeScript' },
  { color: '#00ADD8', Icon: SiGo, label: 'Go' },
  { color: '#CC342D', Icon: SiRuby, label: 'Ruby' },
  { color: '#ED8B00', Icon: SiOpenjdk, label: 'Java' },
  { color: '#6D28D9', label: 'More clients', symbol: '+' },
];

const LLM_PROVIDERS: {
  color: string;
  Icon?: IconType;
  label: string;
  symbol?: string;
}[] = [
  { color: '#111111', Icon: SiOpenai, label: 'OpenAI' },
  { color: '#D97757', Icon: SiClaude, label: 'Claude' },
  { color: '#4285F4', Icon: SiGooglegemini, label: 'Gemini' },
  { color: '#6D28D9', label: 'More providers', symbol: '+' },
];

export function WhyALanguage() {
  const [activeFeatureId, setActiveFeatureId] = useState(FEATURES[0].id);
  const activeFeature =
    FEATURES.find((feature) => feature.id === activeFeatureId) ?? FEATURES[0];

  return (
    <>
      <section
        aria-labelledby="features-heading"
        className="w-full bg-[#FBF7ED]"
      >
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
                Built in{' '}
                <span className="font-semibold text-[#1A1612]">Rust</span> and
                used by some of the world&apos;s largest companies. It has a
                compiler, VM, LSP, formatter, type system with inferred error
                types, and drops into any system so teams can adopt it
                incrementally without rewriting their stack.
              </p>
            </div>

            <div className="relative min-h-[760px] lg:min-h-[720px]">
              <ArchitectureStack
                activeFeature={activeFeature}
                activeFeatureId={activeFeatureId}
                onActiveFeatureChange={setActiveFeatureId}
              />
            </div>
          </div>
        </div>
      </section>

      <style global jsx>{`
        .feature-chip-grid {
          container-name: feature-chip-grid;
          container-type: inline-size;
        }

        @container feature-chip-grid (max-width: 360px) {
          .feature-chip {
            grid-template-columns: 1fr;
            justify-items: center;
            min-height: 58px;
            padding-left: 0.625rem;
            padding-right: 0.625rem;
            text-align: center;
          }

          .feature-chip-label {
            clip: rect(0 0 0 0);
            border: 0;
            height: 1px;
            margin: -1px;
            overflow: hidden;
            padding: 0;
            position: absolute;
            white-space: nowrap;
            width: 1px;
          }
        }
      `}</style>
    </>
  );
}

function ArchitectureStack({
  activeFeature,
  activeFeatureId,
  onActiveFeatureChange,
}: {
  activeFeature: Feature;
  activeFeatureId: string;
  onActiveFeatureChange: (id: string) => void;
}) {
  const ActiveIcon = activeFeature.Icon;

  return (
    <div className="relative h-full lg:translate-x-10 lg:grid lg:grid-cols-[minmax(0,540px)_300px] lg:items-start lg:gap-12">
      <div
        className="relative mx-auto flex max-w-[760px] flex-col items-center gap-10 lg:mx-0 lg:max-w-[540px] lg:gap-0"
        style={{ perspective: '1200px' }}
      >
        <LayerCard
          castsShadow
          className="z-30 w-full lg:h-[250px] lg:w-[108%] lg:-translate-x-6"
          eyebrow="Top Layer"
          floatDelay={0}
          title="Application"
        >
          <div className="flex flex-wrap justify-center gap-4">
            {APP_CLIENTS.map((client) => (
              <IconPill
                color={client.color}
                Icon={client.Icon}
                imageSrc={client.imageSrc}
                key={client.label}
                label={client.label}
                size="large"
                symbol={client.symbol}
              />
            ))}
          </div>
        </LayerCard>

        <LayerCard
          activeTint={activeFeature.tint}
          castsShadow
          className="z-20 -mt-3 w-full lg:-mt-3 lg:h-[250px] lg:w-[108%]"
          eyebrow="Language Layer"
          floatDelay={0.18}
          isActive
          logoSrc="/bamllogopurple.svg"
          title="BAML Language Layer"
        >
          <div className="feature-chip-grid grid w-full grid-cols-2 gap-2 sm:grid-cols-3">
            {FEATURES.map((feature) => (
              <FeatureChip
                feature={feature}
                isActive={feature.id === activeFeatureId}
                key={feature.id}
                onActivate={() => onActiveFeatureChange(feature.id)}
              />
            ))}
          </div>
        </LayerCard>

        <LayerCard
          className="z-10 -mt-3 w-full lg:-mt-3 lg:h-[250px] lg:w-[108%] lg:translate-x-6"
          eyebrow="Bottom Layer"
          floatDelay={0.36}
          title="LLM Infrastructure"
        >
          <div className="flex flex-wrap justify-center gap-4">
            {LLM_PROVIDERS.map((provider) => (
              <IconPill
                color={provider.color}
                Icon={provider.Icon}
                key={provider.label}
                label={provider.label}
                size="large"
                symbol={provider.symbol}
              />
            ))}
          </div>
        </LayerCard>
      </div>

      <motion.aside
        animate={{ opacity: 1, x: 0, y: 0 }}
        className="mt-8 rounded-lg border border-[#D9D3C4] bg-[#FFFCF6]/90 p-5 shadow-[0_22px_70px_-46px_rgba(26,22,18,0.55)] backdrop-blur-md lg:mt-52 lg:w-[300px]"
        initial={{ opacity: 0, x: 10, y: 8 }}
        key={activeFeature.id}
        transition={{ duration: 0.22, ease: [0.22, 0.61, 0.36, 1] }}
      >
        <div className="mb-4 flex items-center gap-3">
          <div
            className="grid size-10 place-items-center rounded-md border"
            style={{
              background: `${activeFeature.tint}12`,
              borderColor: `${activeFeature.tint}38`,
              color: activeFeature.tint,
            }}
          >
            <ActiveIcon aria-hidden size={19} strokeWidth={1.8} />
          </div>
          <div>
            <div className="font-mono text-[10px] uppercase tracking-[0.16em] text-[#8A8580]">
              Feature Detail
            </div>
            <h3 className="text-lg font-semibold leading-tight tracking-[-0.015em] text-[#1A1612]">
              {activeFeature.title}
            </h3>
          </div>
        </div>

        <p className="text-sm leading-6 text-[#5C5852]">{activeFeature.body}</p>

        <div
          className="mt-4 rounded-md border px-3 py-2 text-sm font-semibold"
          style={{
            background: `${activeFeature.tint}0F`,
            borderColor: `${activeFeature.tint}30`,
            color: activeFeature.tint,
          }}
        >
          {activeFeature.outcome}
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          {activeFeature.details.map((detail) => (
            <span
              className="rounded-full border border-[#D9D3C4] bg-[#F4EFE2] px-2.5 py-1 font-mono text-[10px] leading-none tracking-[0.02em] text-[#6B6258]"
              key={detail}
            >
              {detail}
            </span>
          ))}
        </div>
      </motion.aside>
    </div>
  );
}

function LayerCard({
  activeTint,
  castsShadow = false,
  children,
  className,
  eyebrow,
  floatDelay,
  isActive = false,
  logoSrc,
  title,
}: {
  activeTint?: string;
  castsShadow?: boolean;
  children: ReactNode;
  className: string;
  eyebrow: string;
  floatDelay: number;
  isActive?: boolean;
  logoSrc?: string;
  title: string;
}) {
  const restingShadow = castsShadow
    ? '0 34px 60px -42px rgba(26,22,18,0.62), 0 14px 30px -28px rgba(26,22,18,0.42)'
    : '0 1px 0 rgba(255,255,255,0.65) inset';
  const activeShadow = `0 0 0 1px ${activeTint ?? '#6D28D9'}33, 0 34px 60px -42px rgba(26,22,18,0.62), 0 14px 30px -28px rgba(26,22,18,0.42)`;

  return (
    <div
      className={[
        'lg:[transform:rotateX(12deg)_rotateZ(-1.2deg)]',
        className,
      ].join(' ')}
      style={{
        transformStyle: 'preserve-3d',
      }}
    >
      <motion.div
        animate={{
          boxShadow: isActive ? activeShadow : restingShadow,
          y: [0, -7, 0],
        }}
        className={[
          'relative h-full rounded-lg border border-[#D9D3C4] bg-[#FFFCF6]/78 p-5 backdrop-blur-xl',
        ].join(' ')}
        transition={{
          boxShadow: { duration: 0.25 },
          delay: floatDelay,
          duration: 5.2,
          ease: 'easeInOut',
          repeat: Number.POSITIVE_INFINITY,
        }}
      >
        <div
          className="absolute inset-0 rounded-lg opacity-60"
          style={{
            background:
              'linear-gradient(135deg, rgba(255,255,255,0.68), rgba(255,255,255,0.08))',
          }}
        />
        <div className="relative flex h-full flex-col">
          <div className="mb-4 flex items-center justify-between gap-4">
            <div>
              <p className="mb-1 font-mono text-[10px] uppercase tracking-[0.18em] text-[#8A8580]">
                {eyebrow}
              </p>
              <h3 className="text-xl font-semibold tracking-[-0.02em] text-[#1A1612]">
                {title}
              </h3>
            </div>
            {logoSrc ? (
              <motion.div
                animate={{
                  boxShadow: [
                    '0 0 0 1px rgba(167,99,255,0.22), 0 0 14px rgba(167,99,255,0.16)',
                    '0 0 0 1px rgba(167,99,255,0.34), 0 0 22px rgba(167,99,255,0.24)',
                    '0 0 0 1px rgba(167,99,255,0.22), 0 0 14px rgba(167,99,255,0.16)',
                  ],
                }}
                className="grid size-11 shrink-0 place-items-center rounded-md border border-[#A763FF]/30 bg-white/65"
                transition={{
                  duration: 2.8,
                  ease: 'easeInOut',
                  repeat: Number.POSITIVE_INFINITY,
                }}
              >
                <Image
                  alt=""
                  aria-hidden
                  className="h-7 w-7 object-contain"
                  height={28}
                  src={logoSrc}
                  width={28}
                />
              </motion.div>
            ) : null}
          </div>
          <div className="flex w-full flex-1 items-center justify-center">
            {children}
          </div>
        </div>
      </motion.div>
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
      className="feature-chip group grid min-h-[70px] grid-cols-[auto_minmax(0,1fr)] items-center gap-2 overflow-hidden rounded-md border bg-white/80 px-3 py-3 text-left shadow-[0_8px_24px_-22px_rgba(26,22,18,0.45)] transition-[background-color,border-color,box-shadow] hover:bg-white hover:shadow-[0_16px_34px_-26px_rgba(26,22,18,0.55)] focus:outline-none focus:ring-2 focus:ring-[#6D28D9]/25"
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
        className="grid size-8 shrink-0 place-items-center rounded-md border"
        style={{
          background: `${feature.tint}12`,
          borderColor: `${feature.tint}33`,
          color: feature.tint,
        }}
      >
        <Icon aria-hidden size={16} strokeWidth={1.9} />
      </span>
      <span className="feature-chip-label min-w-0">
        <span className="block text-[13px] font-semibold leading-tight text-[#1A1612]">
          {feature.title}
        </span>
      </span>
    </button>
  );
}

function IconPill({
  color,
  Icon,
  imageSrc,
  label,
  size = 'default',
  symbol,
}: {
  color: string;
  Icon?: IconType;
  imageSrc?: string;
  label: string;
  size?: 'default' | 'large';
  symbol?: string;
}) {
  const isLarge = size === 'large';

  return (
    <span
      aria-label={label}
      className={[
        'grid place-items-center rounded-full border border-[#D9D3C4] bg-white/65',
        isLarge ? 'size-14' : 'size-10',
      ].join(' ')}
      role="img"
      title={label}
    >
      {imageSrc ? (
        <Image
          alt=""
          aria-hidden
          height={isLarge ? 28 : 20}
          src={imageSrc}
          width={isLarge ? 28 : 20}
        />
      ) : Icon ? (
        <Icon aria-hidden size={isLarge ? 25 : 18} style={{ color }} />
      ) : (
        <span
          aria-hidden
          className={[
            'font-mono font-semibold leading-none',
            isLarge ? 'text-2xl' : 'text-lg',
          ].join(' ')}
          style={{ color }}
        >
          {symbol}
        </span>
      )}
    </span>
  );
}
