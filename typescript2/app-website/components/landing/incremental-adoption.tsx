'use client';

import {
  AnimatePresence,
  type MotionValue,
  motion,
  useMotionValueEvent,
  useReducedMotion,
  useScroll,
  useTransform,
} from 'motion/react';
import { useEffect, useRef, useState } from 'react';

// Tokens
const BG = '#ffffff';
const BORDER = '#D9D3C4';
const ACCENT = '#7C3AED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
const HAND = '"Helvetica Neue", Helvetica, Arial, sans-serif';

// Code geometry — must match CodeBlock styles below.
const TAB_HEIGHT = 30;
const CODE_PAD_TOP = 12;
const CODE_PAD_LEFT = 16;
const LINE_HEIGHT = 20;
const lineCenterY = (n: number) =>
  TAB_HEIGHT + 1 + CODE_PAD_TOP + (n - 0.5) * LINE_HEIGHT;

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

type Annotation = {
  text: string;
  lineNumber: number;
  column: number;
};

type BlockSpec = {
  key: string;
  code: string;
  lang: 'python' | 'baml';
  filename: string;
  scale?: number;
  annotations: Annotation[];
};

type Step = {
  heading: string;
  body: string;
  blocks: BlockSpec[];
};

const STEPS: Step[] = [
  {
    heading: 'Every agent codebase starts here.',
    body: "A string prompt. Manual JSON parsing. No types, no retries, no guarantees. Works until it doesn't.",
    blocks: [
      {
        key: 's1-py',
        code: STEP_1_PY,
        lang: 'python',
        filename: 'main.py',
        annotations: [
          {
            text: 'string prompts.\nJSON.loads and hope.',
            lineNumber: 11,
            column: 26,
          },
          {
            text: 'silent failure when\nJSON is malformed',
            lineNumber: 22,
            column: 16,
          },
        ],
      },
    ],
  },
  {
    heading: 'Types help. But the prompt is still a string.',
    body: 'Pydantic validates after the model responds. If the JSON is wrong, you find out at runtime. The model is still guessing at what you want.',
    blocks: [
      {
        key: 's2-py',
        code: STEP_2_PY,
        lang: 'python',
        filename: 'main.py',
        annotations: [
          {
            text: 'types — on your side',
            lineNumber: 11,
            column: 22,
          },
          {
            text: 'but the model still\ngets a string.',
            lineNumber: 23,
            column: 28,
          },
        ],
      },
    ],
  },
  {
    heading:
      'Define the schema and prompt once. Call it from your existing code.',
    body: 'Schema, prompt, and model choice — all in one place. Your Python app stays Python. BAML handles the LLM boundary.',
    blocks: [
      {
        key: 's3-baml',
        code: STEP_3_BAML,
        lang: 'baml',
        filename: 'triage.baml',
        annotations: [
          {
            text: 'BAML injects the schema.\nthe model knows what to return.',
            lineNumber: 18,
            column: 28,
          },
        ],
      },
      {
        key: 's3-py',
        code: STEP_3_PY,
        lang: 'python',
        filename: 'main.py',
        annotations: [
          {
            text: 'your existing app,\nmostly unchanged',
            lineNumber: 4,
            column: 28,
          },
        ],
      },
    ],
  },
  {
    heading: "This is why it's a language.",
    body: 'Retries, streaming, tests, multi-language codegen — compiler-level, not library-level. One file, every client, every guarantee.',
    blocks: [
      {
        key: 's4-baml',
        code: STEP_4_BAML,
        lang: 'baml',
        filename: 'triage.baml',
        annotations: [
          {
            text: 'declarative retries.\nnot a library wrapper.',
            lineNumber: 18,
            column: 24,
          },
          {
            text: 'tests live with the prompt.\nrun them in the playground or CI.',
            lineNumber: 23,
            column: 18,
          },
        ],
      },
    ],
  },
];

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

// ── Code block — renders Shiki HTML with a shared style ──────────────────────

function CodeBlock({
  html,
  filename,
}: {
  html: string;
  filename: string;
}) {
  return (
    <div
      style={{
        borderRadius: 8,
        border: `1px solid ${BORDER}`,
        background: '#FBF8F1',
        boxShadow: '0 1px 0 rgba(0,0,0,0.02), 0 6px 24px -16px rgba(0,0,0,0.18)',
        overflow: 'hidden',
        width: '100%',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          height: TAB_HEIGHT,
          boxSizing: 'border-box',
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
          lineHeight: `${LINE_HEIGHT}px`,
          padding: `${CODE_PAD_TOP}px ${CODE_PAD_LEFT}px`,
          overflow: 'auto',
        }}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}

// ── Annotated block: code on the left, caption gutter on the right ───────────

function AnnotatedBlock({ html, block }: { html: string; block: BlockSpec }) {
  const GAP = 16;
  const transform = block.scale ? `scale(${block.scale})` : undefined;

  return (
    <div
      style={{
        position: 'relative',
        display: 'grid',
        gridTemplateColumns: '1.6fr 1fr',
        columnGap: GAP,
        width: '100%',
        transform,
        transformOrigin: 'top left',
      }}
    >
      <div style={{ position: 'relative', minWidth: 0, width: '100%' }}>
        <CodeBlock html={html} filename={block.filename} />
        {/* Line highlights — one per annotation */}
        {block.annotations.map((a, i) => (
          <motion.div
            key={`${block.key}-hl-${i}`}
            initial={{ opacity: 0, scaleX: 0.98 }}
            animate={{ opacity: 1, scaleX: 1 }}
            transition={{ duration: 0.35, delay: 0.15 + i * 0.08 }}
            style={{
              position: 'absolute',
              left: 1,
              right: 1,
              top: lineCenterY(a.lineNumber) - LINE_HEIGHT / 2,
              height: LINE_HEIGHT,
              background: 'rgba(124, 58, 237, 0.14)',
              borderLeft: `2px solid ${ACCENT}`,
              transformOrigin: 'left center',
              pointerEvents: 'none',
            }}
          />
        ))}
      </div>
      <div style={{ position: 'relative' }}>
        {block.annotations.map((a, i) => (
          <motion.div
            key={`${block.key}-${i}`}
            initial={{ opacity: 0, x: -4 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.35, delay: 0.25 + i * 0.08 }}
            style={{
              position: 'absolute',
              top: lineCenterY(a.lineNumber),
              left: 12,
              right: 0,
              transform: 'translateY(-50%)',
              fontFamily: HAND,
              fontStyle: 'normal',
              fontSize: 14,
              fontWeight: 400,
              lineHeight: 1.4,
              letterSpacing: '0.01em',
              color: ACCENT,
              opacity: 0.85,
              whiteSpace: 'pre-line',
              pointerEvents: 'none',
            }}
          >
            {a.text}
          </motion.div>
        ))}
      </div>
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

  const fadeT = reduced ? { duration: 0 } : { duration: 0.35 };

  const blockHtmlByKey: Record<string, string> = {
    's1-py': html1,
    's2-py': html2,
    's3-baml': htmlBaml3,
    's3-py': htmlPy3,
    's4-baml': htmlBaml4,
  };

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
        <motion.div
          key={`step-${activeStep}`}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={fadeT}
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            flexDirection: 'column',
            gap: 12,
          }}
        >
          {STEPS[activeStep].blocks.map((b) => (
            <div
              key={b.key}
              style={{
                flex:
                  STEPS[activeStep].blocks.length > 1 ? '1 1 0' : '0 0 auto',
                minHeight: 0,
                display: 'flex',
              }}
            >
              <AnnotatedBlock block={b} html={blockHtmlByKey[b.key]} />
            </div>
          ))}
          {activeStep === 3 && (
            <div
              style={{
                marginTop: 8,
                fontFamily: 'var(--font-serif)',
                fontStyle: 'italic',
                fontSize: 14,
                color: MUTED,
                textAlign: 'center',
              }}
            >
              the same .baml file generates python, typescript, ruby, and go
              clients.
            </div>
          )}
        </motion.div>
      </AnimatePresence>

      <StepPills activeStep={activeStep} />
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
            key={`pill-${i}`}
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

// ── Per-step body text streamer ───────────────────────────────────────────────

function TypingText({
  text,
  progress,
}: {
  text: string;
  progress: MotionValue<number>;
}) {
  const [chars, setChars] = useState(0);

  useEffect(() => {
    const update = () => {
      const v = progress.get();
      const next = Math.max(0, Math.min(text.length, Math.round(v * text.length)));
      setChars(next);
    };
    update();
    const unsub = progress.on('change', update);
    return unsub;
  }, [progress, text.length]);

  return (
    <>
      <span>{text.slice(0, chars)}</span>
      <span aria-hidden style={{ opacity: 0 }}>
        {text.slice(chars)}
      </span>
    </>
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

  // Per-step body typewriter progress (0 = empty, 1 = fully typed).
  // Each step types in just before its active range and types out just before
  // the next step takes over, giving the "characters streaming" effect.
  const step0Progress = useTransform(
    scrollYProgress,
    [0, 0.08, 0.12, 0.22],
    [1, 1, 1, 0],
  );
  const step1Progress = useTransform(
    scrollYProgress,
    [0.22, 0.32, 0.38, 0.48],
    [0, 1, 1, 0],
  );
  const step2Progress = useTransform(
    scrollYProgress,
    [0.48, 0.58, 0.64, 0.74],
    [0, 1, 1, 0],
  );
  const step3Progress = useTransform(
    scrollYProgress,
    [0.74, 0.86, 1],
    [0, 1, 1],
  );
  const stepProgresses = [
    step0Progress,
    step1Progress,
    step2Progress,
    step3Progress,
  ];

  useMotionValueEvent(scrollYProgress, 'change', (latest) => {
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
          gridTemplateColumns: '40% 60%',
          gap: 48,
        }}
      >
        <div>
          {STEPS.map((s, i) => {
            const isActive = i === activeStep;
            const STACK_TOP_BASE = 80;
            const STACK_TAB_HEIGHT = 56;
            const stickyTop = STACK_TOP_BASE + i * STACK_TAB_HEIGHT;
            return (
              <section
                key={`step-text-${i}`}
                style={{
                  minHeight: '80vh',
                  paddingLeft: 24,
                  paddingBottom: 32,
                }}
              >
                <div
                  style={{
                    position: 'sticky',
                    top: stickyTop,
                    background: BG,
                    paddingTop: 14,
                    paddingBottom: 14,
                    paddingRight: 16,
                    zIndex: STEPS.length - i,
                    borderTop: `1px solid ${BORDER}`,
                    transition: 'opacity 350ms ease',
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'baseline',
                      gap: 16,
                    }}
                  >
                    <div
                      style={{
                        fontSize: 11,
                        fontWeight: 500,
                        letterSpacing: '0.14em',
                        textTransform: 'uppercase',
                        color: isActive ? ACCENT : '#8A8580',
                        flexShrink: 0,
                        transition: 'color 350ms ease',
                      }}
                    >
                      Step {String(i + 1).padStart(2, '0')}
                    </div>
                    <h3
                      style={{
                        fontSize: 'clamp(1.25rem, 2vw, 1.65rem)',
                        fontWeight: 600,
                        lineHeight: 1.15,
                        letterSpacing: '-0.02em',
                        margin: 0,
                        color: isActive ? INK : MUTED,
                        transition: 'color 350ms ease',
                      }}
                    >
                      {s.heading}
                    </h3>
                  </div>
                </div>
                <p
                  style={{
                    marginTop: 20,
                    fontSize: 16,
                    lineHeight: 1.6,
                    color: MUTED,
                    maxWidth: 440,
                  }}
                >
                  <TypingText text={s.body} progress={stepProgresses[i]} />
                </p>
              </section>
            );
          })}
        </div>

        <div
          style={{
            position: 'sticky',
            top: 'calc(var(--navigation-height, 56px) + 32px)',
            alignSelf: 'start',
            height: 'min(640px, 82vh)',
          }}
        >
          <StickyPanel activeStep={activeStep} />
        </div>
      </div>

      <style>{`
        .adoption-code pre { margin: 0; background: transparent !important; }
        .adoption-code code { background: transparent !important; font-family: ${MONO} !important; }
      `}</style>
    </section>
  );
}
