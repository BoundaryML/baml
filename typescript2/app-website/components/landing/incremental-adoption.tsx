'use client';

import {
  AnimatePresence,
  motion,
  useMotionValueEvent,
  useReducedMotion,
  useScroll,
} from 'motion/react';
import { useEffect, useRef, useState } from 'react';

// Tokens
const BG = '#F5F1E8';
const BORDER = '#D9D3C4';
const ACCENT = '#7C3AED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
const HAND = '"Caveat", cursive';

// Code sources — rendered verbatim
const STEP_1_PY = `from openai import OpenAI
import json

client = OpenAI()

def triage(message: str):
    response = client.chat.completions.create(
        model="gpt-4",
        messages=[{
            "role": "user",
            "content": f"""Classify this support message and extract info.
Return JSON with: category (billing/technical/feedback),
order_id (if mentioned), sentiment (-1 to 1), urgency (low/med/high),
next_action (object with type and reason).

Message: {message}"""
        }]
    )
    try:
        return json.loads(response.choices[0].message.content)
    except json.JSONDecodeError:
        return None  # just give up
`;

const STEP_2_PY = `from openai import OpenAI
from pydantic import BaseModel, ValidationError
from typing import Literal, Optional

client = OpenAI()

class NextAction(BaseModel):
    type: Literal["auto_reply", "escalate", "create_ticket"]
    reason: str

class Triage(BaseModel):
    category: Literal["billing", "technical", "feedback"]
    order_id: Optional[str]
    sentiment: float
    urgency: Literal["low", "med", "high"]
    next_action: NextAction

def triage(message: str) -> Triage | None:
    response = client.chat.completions.create(
        model="gpt-4",
        messages=[{
            "role": "user",
            "content": f"""Classify this support message...
            (same string prompt as before)"""
        }]
    )
    try:
        return Triage.model_validate_json(
            response.choices[0].message.content
        )
    except ValidationError:
        return None
`;

const STEP_3_BAML = `class NextAction {
  type "auto_reply" | "escalate" | "create_ticket"
  reason string
}

class Triage {
  category "billing" | "technical" | "feedback"
  order_id string?
  sentiment float
  urgency "low" | "med" | "high"
  next_action NextAction
}

function Triage(message: string) -> Triage {
  client GPT4
  prompt #"
    Classify this support message and extract info.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ message }}
  "#
}
`;

const STEP_3_PY = `from baml_client import b

def triage(message: str) -> Triage:
    return b.Triage(message)
`;

const STEP_4_BAML = `class Triage {
  category "billing" | "technical" | "feedback"
  order_id string?
  sentiment float @description("from -1 to 1")
  urgency "low" | "med" | "high"
  next_action NextAction
}

function Triage(message: string) -> Triage {
  client GPT4
  prompt #"
    Classify this support message and extract info.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ message }}
  "#
}

retry_policy Exponential {
  max_retries 3
  strategy { type exponential_backoff }
}

test BillingQuery {
  functions [Triage]
  args {
    message "I was charged twice for order #4402 last week"
  }
  @@assert(is_billing, {{ this.category == "billing" }})
  @@assert(has_order, {{ this.order_id == "4402" }})
}
`;

const STEPS = [
  {
    heading: "Every agent codebase starts here.",
    body:
      "A string prompt. Manual JSON parsing. No types, no retries, no guarantees. Works until it doesn't.",
  },
  {
    heading: "Types help. But the prompt is still a string.",
    body:
      "Pydantic validates after the model responds. If the JSON is wrong, you find out at runtime. The model is still guessing at what you want.",
  },
  {
    heading:
      "Define the schema and prompt once. Call it from your existing code.",
    body:
      "Schema, prompt, and model choice — all in one place. Your Python app stays Python. BAML handles the LLM boundary.",
  },
  {
    heading: "This is why it's a language.",
    body:
      "Retries, streaming, tests, multi-language codegen — compiler-level, not library-level. One file, every client, every guarantee.",
  },
];

// Annotations — positioned in % relative to the code panel bounds
type Annotation = {
  text: string;
  // Label position (top-left corner) in % of panel
  labelX: number;
  labelY: number;
  // Arrow end (where it points) in % of panel
  targetX: number;
  targetY: number;
  // Curve bend direction: +1 bows right/down, -1 bows left/up
  bend: number;
};

const ANNOTATIONS: Record<number, Annotation[]> = {
  0: [
    {
      text: 'string prompts. JSON.loads and hope.',
      labelX: 62,
      labelY: 34,
      targetX: 44,
      targetY: 44,
      bend: 1,
    },
    {
      text: 'silent failure when JSON is malformed',
      labelX: 60,
      labelY: 82,
      targetX: 40,
      targetY: 88,
      bend: -1,
    },
  ],
  1: [
    {
      text: 'types — on your side',
      labelX: 4,
      labelY: 2,
      targetX: 22,
      targetY: 24,
      bend: 1,
    },
    {
      text: "but the model still gets a string.\nit doesn't know your schema.",
      labelX: 58,
      labelY: 62,
      targetX: 42,
      targetY: 70,
      bend: -1,
    },
  ],
  2: [
    {
      text: 'BAML injects the schema.\nthe model knows what to return.',
      labelX: 58,
      labelY: 60,
      targetX: 32,
      targetY: 66,
      bend: 1,
    },
    {
      text: 'your existing app, mostly unchanged',
      labelX: 4,
      labelY: 90,
      targetX: 30,
      targetY: 94,
      bend: -1,
    },
  ],
  3: [
    {
      text: 'declarative retries.\nnot a library wrapper.',
      labelX: 58,
      labelY: 58,
      targetX: 34,
      targetY: 66,
      bend: 1,
    },
    {
      text: 'tests live with the prompt.\nrun them in the playground or CI.',
      labelX: 4,
      labelY: 80,
      targetX: 30,
      targetY: 88,
      bend: -1,
    },
  ],
};

// ── Shiki hook ────────────────────────────────────────────────────────────────

type HighlightInput = { code: string; lang: 'python' | 'baml' };

function useHighlighted(inputs: HighlightInput[]): string[] {
  const [out, setOut] = useState<string[]>(() =>
    inputs.map((i) => `<pre><code>${escapeHtml(i.code)}</code></pre>`),
  );

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const { createHighlighter } = await import('shiki');
        const { bamlTextmate, bamlJinjaTextmate } = await import(
          '@/lib/mdx/shiki-grammars'
        );
        const highlighter = await createHighlighter({
          themes: ['github-light'],
          langs: ['python', bamlJinjaTextmate, bamlTextmate],
        });
        const results = inputs.map((i) =>
          highlighter.codeToHtml(i.code, {
            lang: i.lang,
            theme: 'github-light',
          }),
        );
        if (!cancelled) setOut(results);
      } catch {
        /* fall back to plain pre */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return out;
}

function escapeHtml(s: string): string {
  return s
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');
}

// ── SVG arrow (hand-drawn feel) ───────────────────────────────────────────────

function ArrowSVG({ annotation }: { annotation: Annotation }) {
  const { labelX, labelY, targetX, targetY, bend } = annotation;
  // Start near the label's leading edge
  const sx = labelX + (targetX > labelX ? 0 : 18);
  const sy = labelY + 4;
  const ex = targetX;
  const ey = targetY;
  // Two control points with slight wobble for hand-drawn feel
  const midX = (sx + ex) / 2;
  const midY = (sy + ey) / 2;
  const offset = 10 * bend;
  const c1x = midX + offset;
  const c1y = sy + offset * 0.6;
  const c2x = midX - offset * 0.4;
  const c2y = ey - offset * 0.3;
  const d = `M ${sx} ${sy} C ${c1x} ${c1y}, ${c2x} ${c2y}, ${ex} ${ey}`;

  // Arrow head — small, rotated to match endpoint tangent
  const dx = ex - c2x;
  const dy = ey - c2y;
  const angle = (Math.atan2(dy, dx) * 180) / Math.PI;

  return (
    <svg
      aria-hidden
      style={{
        position: 'absolute',
        inset: 0,
        width: '100%',
        height: '100%',
        pointerEvents: 'none',
      }}
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
    >
      <motion.path
        d={d}
        fill="none"
        stroke={ACCENT}
        strokeWidth={0.45}
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
        initial={{ pathLength: 0 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 0.7, ease: [0.65, 0, 0.35, 1] }}
      />
      <motion.g
        transform={`translate(${ex} ${ey}) rotate(${angle})`}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.6, duration: 0.2 }}
      >
        <path
          d="M 0 0 L -2.4 -1.2 M 0 0 L -2.4 1.2"
          stroke={ACCENT}
          strokeWidth={0.5}
          strokeLinecap="round"
          fill="none"
          vectorEffect="non-scaling-stroke"
        />
      </motion.g>
    </svg>
  );
}

// ── Annotation label ──────────────────────────────────────────────────────────

function AnnotationLabel({ annotation }: { annotation: Annotation }) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.3, delay: 0.25 }}
      style={{
        position: 'absolute',
        left: `${annotation.labelX}%`,
        top: `${annotation.labelY}%`,
        maxWidth: 200,
        fontFamily: HAND,
        fontSize: 18,
        lineHeight: 1.15,
        color: ACCENT,
        whiteSpace: 'pre-line',
        pointerEvents: 'none',
        textShadow: `0 0 6px ${BG}`,
      }}
    >
      {annotation.text}
    </motion.div>
  );
}

// ── Code block — renders Shiki HTML with a shared style ───────────────────────

function CodeBlock({
  html,
  filename,
  scale = 1,
}: {
  html: string;
  filename: string;
  scale?: number;
}) {
  return (
    <div
      style={{
        borderRadius: 8,
        border: `1px solid ${BORDER}`,
        background: '#FBF8F1',
        boxShadow: '0 1px 0 rgba(0,0,0,0.02), 0 6px 24px -16px rgba(0,0,0,0.18)',
        overflow: 'hidden',
        transform: scale !== 1 ? `scale(${scale})` : undefined,
        transformOrigin: 'top left',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          borderBottom: `1px solid ${BORDER}`,
          fontFamily: MONO,
          fontSize: 11,
          letterSpacing: '0.04em',
          color: MUTED,
          background: '#F3EFE3',
        }}
      >
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: '#E6DFCC',
          }}
        />
        {filename}
      </div>
      <div
        className="adoption-code"
        style={{
          fontFamily: MONO,
          fontSize: 12.5,
          lineHeight: 1.6,
          padding: '12px 16px',
          overflow: 'auto',
        }}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}

// ── Sticky code panel with per-step transitions ───────────────────────────────

function StickyPanel({ activeStep }: { activeStep: number }) {
  const reduced = useReducedMotion();
  const [html1, html2, htmlBaml3, htmlPy3, htmlBaml4] = useHighlighted([
    { code: STEP_1_PY, lang: 'python' },
    { code: STEP_2_PY, lang: 'python' },
    { code: STEP_3_BAML, lang: 'baml' },
    { code: STEP_3_PY, lang: 'python' },
    { code: STEP_4_BAML, lang: 'baml' },
  ]);

  const fade = reduced
    ? { duration: 0 }
    : { duration: 0.35, ease: [0.4, 0, 0.2, 1] };

  return (
    <div
      style={{
        position: 'relative',
        width: '100%',
        height: '100%',
        minHeight: 520,
      }}
    >
      <AnimatePresence initial={false} mode="popLayout">
        {activeStep === 0 && (
          <motion.div
            key="s1"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fade}
            style={{ position: 'absolute', inset: 0 }}
          >
            <CodeBlock html={html1} filename="main.py" />
            <AnnotationLayer step={0} />
          </motion.div>
        )}

        {activeStep === 1 && (
          <motion.div
            key="s2"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{
              opacity: 0,
              scaleY: reduced ? 1 : 0.18,
              transformOrigin: 'top',
            }}
            transition={reduced ? { duration: 0 } : { duration: 0.5 }}
            style={{ position: 'absolute', inset: 0 }}
          >
            <CodeBlock html={html2} filename="main.py" />
            <AnnotationLayer step={1} />
          </motion.div>
        )}

        {activeStep === 2 && (
          <motion.div
            key="s3"
            initial={reduced ? { opacity: 1 } : { opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={reduced ? { duration: 0 } : { duration: 0.5, delay: 0.15 }}
            style={{
              position: 'absolute',
              inset: 0,
              display: 'flex',
              flexDirection: 'column',
              gap: 12,
            }}
          >
            <motion.div
              initial={reduced ? {} : { y: -16, opacity: 0 }}
              animate={{ y: 0, opacity: 1 }}
              transition={reduced ? { duration: 0 } : { duration: 0.45, delay: 0.2 }}
            >
              <CodeBlock html={htmlBaml3} filename="triage.baml" />
            </motion.div>
            <motion.div
              initial={reduced ? {} : { opacity: 0 }}
              animate={{ opacity: 1 }}
              transition={reduced ? { duration: 0 } : { duration: 0.3, delay: 0.5 }}
            >
              <CodeBlock html={htmlPy3} filename="main.py" scale={0.92} />
            </motion.div>
            <AnnotationLayer step={2} />
          </motion.div>
        )}

        {activeStep === 3 && (
          <motion.div
            key="s4"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fade}
            style={{ position: 'absolute', inset: 0 }}
          >
            <CodeBlock html={htmlBaml4} filename="triage.baml" />
            <div
              style={{
                marginTop: 10,
                fontFamily: HAND,
                fontSize: 16,
                color: MUTED,
                textAlign: 'center',
              }}
            >
              the same .baml file generates python, typescript, ruby, and go
              clients.
            </div>
            <AnnotationLayer step={3} />
          </motion.div>
        )}
      </AnimatePresence>

      <StepPills activeStep={activeStep} />
    </div>
  );
}

function AnnotationLayer({ step }: { step: number }) {
  const notes = ANNOTATIONS[step] ?? [];
  return (
    <div
      style={{
        position: 'absolute',
        inset: 0,
        pointerEvents: 'none',
      }}
    >
      {notes.map((n, i) => (
        <div
          key={i}
          style={{ position: 'absolute', inset: 0, pointerEvents: 'none' }}
        >
          <ArrowSVG annotation={n} />
          <AnnotationLabel annotation={n} />
        </div>
      ))}
    </div>
  );
}

function StepPills({ activeStep }: { activeStep: number }) {
  return (
    <div
      style={{
        position: 'absolute',
        bottom: -32,
        left: 0,
        right: 0,
        display: 'flex',
        justifyContent: 'center',
        gap: 8,
      }}
    >
      {STEPS.map((_, i) => {
        const active = i === activeStep;
        return (
          <span
            key={i}
            style={{
              width: active ? 22 : 10,
              height: 6,
              borderRadius: 999,
              background: active ? ACCENT : 'transparent',
              border: `1px solid ${active ? ACCENT : BORDER}`,
              transition: 'width 200ms ease, background-color 200ms ease',
            }}
          />
        );
      })}
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function IncrementalAdoption() {
  const ref = useRef<HTMLDivElement>(null);
  const { scrollYProgress } = useScroll({
    target: ref,
    offset: ['start start', 'end end'],
  });
  const [activeStep, setActiveStep] = useState(0);

  useMotionValueEvent(scrollYProgress, 'change', (latest) => {
    // Map 0→1 across the container onto 4 steps with bias toward the middle
    // so each step has similar dwell time.
    let idx = 0;
    if (latest < 0.22) idx = 0;
    else if (latest < 0.48) idx = 1;
    else if (latest < 0.74) idx = 2;
    else idx = 3;
    if (idx !== activeStep) setActiveStep(idx);
  });

  return (
    <section
      ref={ref}
      aria-label="Incremental BAML adoption"
      style={{
        background: BG,
        color: INK,
        padding: '120px 0 160px',
        width: '100%',
      }}
    >
      <div
        style={{
          maxWidth: 1200,
          margin: '0 auto',
          padding: '0 32px 0 48px',
          marginBottom: 40,
        }}
      >
        <p
          style={{
            fontSize: 13,
            fontWeight: 500,
            letterSpacing: '0.12em',
            textTransform: 'uppercase',
            color: '#8A8580',
            margin: 0,
          }}
        >
          Adopt BAML gradually
        </p>
        <h2
          style={{
            fontSize: 'clamp(2rem, 4vw, 3rem)',
            fontWeight: 600,
            lineHeight: 1.05,
            letterSpacing: '-0.03em',
            margin: '12px 0 0',
          }}
        >
          From f-strings to a language, one step at a time.
        </h2>
      </div>

      <div
        style={{
          maxWidth: 1200,
          margin: '0 auto',
          padding: '0 32px 0 48px',
          display: 'grid',
          gridTemplateColumns: '60% 40%',
          gap: 48,
        }}
      >
        <div
          style={{
            position: 'sticky',
            top: 'calc(var(--navigation-height, 56px) + 32px)',
            alignSelf: 'start',
            height: 'min(620px, 80vh)',
          }}
        >
          <StickyPanel activeStep={activeStep} />
        </div>

        <div>
          {STEPS.map((s, i) => (
            <section
              key={i}
              style={{
                minHeight: '100vh',
                display: 'flex',
                flexDirection: 'column',
                justifyContent: 'center',
                paddingRight: 24,
              }}
            >
              <div
                style={{
                  fontSize: 12,
                  fontWeight: 500,
                  letterSpacing: '0.14em',
                  textTransform: 'uppercase',
                  color: '#8A8580',
                  marginBottom: 12,
                }}
              >
                Step {String(i + 1).padStart(2, '0')}
              </div>
              <h3
                style={{
                  fontSize: 'clamp(1.6rem, 2.5vw, 2.25rem)',
                  fontWeight: 600,
                  lineHeight: 1.08,
                  letterSpacing: '-0.03em',
                  margin: 0,
                }}
              >
                {s.heading}
              </h3>
              <p
                style={{
                  marginTop: 16,
                  fontSize: 16,
                  lineHeight: 1.6,
                  color: MUTED,
                  maxWidth: 440,
                }}
              >
                {s.body}
              </p>
            </section>
          ))}
        </div>
      </div>

      <style>{`
        .adoption-code pre { margin: 0; background: transparent !important; }
        .adoption-code code { background: transparent !important; font-family: ${MONO} !important; }
      `}</style>
    </section>
  );
}
