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
    blocks: [
      {
        annotations: [
          {
            column: 26,
            lineNumber: 11,
            text: 'string prompts.\nJSON.loads and hope.',
          },
          {
            column: 16,
            lineNumber: 22,
            text: 'silent failure when\nJSON is malformed',
          },
        ],
        code: STEP_1_PY,
        filename: 'main.py',
        key: 's1-py',
        lang: 'python',
      },
    ],
    body: "A string prompt. Manual JSON parsing. No types, no retries, no guarantees. Works until it doesn't.",
    heading: 'Every agent codebase starts here.',
  },
  {
    blocks: [
      {
        annotations: [
          {
            column: 22,
            lineNumber: 11,
            text: 'types, on your side',
          },
          {
            column: 28,
            lineNumber: 23,
            text: 'but the model still\ngets a string.',
          },
        ],
        code: STEP_2_PY,
        filename: 'main.py',
        key: 's2-py',
        lang: 'python',
      },
    ],
    body: 'Pydantic validates after the model responds. If the JSON is wrong, you find out at runtime. The model is still guessing at what you want.',
    heading: 'Types help. But the prompt is still a string.',
  },
  {
    blocks: [
      {
        annotations: [
          {
            column: 28,
            lineNumber: 18,
            text: 'BAML injects the schema.\nthe model knows what to return.',
          },
        ],
        code: STEP_3_BAML,
        filename: 'triage.baml',
        key: 's3-baml',
        lang: 'baml',
      },
      {
        annotations: [
          {
            column: 28,
            lineNumber: 4,
            text: 'your existing app,\nmostly unchanged',
          },
        ],
        code: STEP_3_PY,
        filename: 'main.py',
        key: 's3-py',
        lang: 'python',
      },
    ],
    body: 'Schema, prompt, and model choice in one place. Your Python app stays Python. BAML handles the LLM boundary.',
    heading:
      'Define the schema and prompt once. Call it from your existing code.',
  },
  {
    blocks: [
      {
        annotations: [
          {
            column: 24,
            lineNumber: 18,
            text: 'declarative retries.\nnot a library wrapper.',
          },
          {
            column: 18,
            lineNumber: 23,
            text: 'tests live with the prompt.\nrun them in the playground or CI.',
          },
        ],
        code: STEP_4_BAML,
        filename: 'triage.baml',
        key: 's4-baml',
        lang: 'baml',
      },
    ],
    body: 'Retries, streaming, tests, and multi-language codegen at compiler level, not library level. One file, every client, every guarantee.',
    heading: 'Yeah, we built a whole language.',
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
          langs: ['python', bamlJinjaTextmate, bamlTextmate],
          themes: ['github-light'],
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

function CodeBlock({ html, filename }: { html: string; filename: string }) {
  return (
    <div
      style={{
        background: '#FBF8F1',
        border: `1px solid ${BORDER}`,
        borderRadius: 8,
        boxShadow:
          '0 1px 0 rgba(0,0,0,0.02), 0 6px 24px -16px rgba(0,0,0,0.18)',
        overflow: 'hidden',
        width: '100%',
      }}
    >
      <div
        style={{
          alignItems: 'center',
          background: '#F3EFE3',
          borderBottom: `1px solid ${BORDER}`,
          boxSizing: 'border-box',
          color: MUTED,
          display: 'flex',
          fontFamily: MONO,
          fontSize: 11,
          gap: 8,
          height: TAB_HEIGHT,
          letterSpacing: '0.04em',
          padding: '8px 12px',
        }}
      >
        <span
          style={{
            background: '#E6DFCC',
            borderRadius: '50%',
            height: 8,
            width: 8,
          }}
        />
        {filename}
      </div>
      <div
        className="adoption-code"
        dangerouslySetInnerHTML={{ __html: html }}
        style={{
          fontFamily: MONO,
          fontSize: 12.5,
          lineHeight: `${LINE_HEIGHT}px`,
          overflow: 'auto',
          padding: `${CODE_PAD_TOP}px ${CODE_PAD_LEFT}px`,
        }}
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
        columnGap: GAP,
        display: 'grid',
        gridTemplateColumns: '1.6fr 1fr',
        position: 'relative',
        transform,
        transformOrigin: 'top left',
        width: '100%',
      }}
    >
      <div style={{ minWidth: 0, position: 'relative', width: '100%' }}>
        <CodeBlock filename={block.filename} html={html} />
        {/* Line highlights — one per annotation */}
        {block.annotations.map((a, i) => (
          <motion.div
            animate={{ opacity: 1, scaleX: 1 }}
            initial={{ opacity: 0, scaleX: 0.98 }}
            key={`${block.key}-hl-${i}`}
            style={{
              background: 'rgba(124, 58, 237, 0.14)',
              borderLeft: `2px solid ${ACCENT}`,
              height: LINE_HEIGHT,
              left: 1,
              pointerEvents: 'none',
              position: 'absolute',
              right: 1,
              top: lineCenterY(a.lineNumber) - LINE_HEIGHT / 2,
              transformOrigin: 'left center',
            }}
            transition={{ delay: 0.15 + i * 0.08, duration: 0.35 }}
          />
        ))}
      </div>
      <div style={{ position: 'relative' }}>
        {block.annotations.map((a, i) => (
          <motion.div
            animate={{ opacity: 1, x: 0 }}
            initial={{ opacity: 0, x: -4 }}
            key={`${block.key}-${i}`}
            style={{
              color: ACCENT,
              fontFamily: HAND,
              fontSize: 14,
              fontStyle: 'normal',
              fontWeight: 400,
              left: 12,
              letterSpacing: '0.01em',
              lineHeight: 1.4,
              opacity: 0.85,
              pointerEvents: 'none',
              position: 'absolute',
              right: 0,
              top: lineCenterY(a.lineNumber),
              transform: 'translateY(-50%)',
              whiteSpace: 'pre-line',
            }}
            transition={{ delay: 0.25 + i * 0.08, duration: 0.35 }}
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
        height: '100%',
        minHeight: 520,
        position: 'relative',
        width: '100%',
      }}
    >
      <AnimatePresence initial={false} mode="popLayout">
        <motion.div
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          initial={{ opacity: 0 }}
          key={`step-${activeStep}`}
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 12,
            inset: 0,
            position: 'absolute',
          }}
          transition={fadeT}
        >
          {STEPS[activeStep].blocks.map((b) => (
            <div
              key={b.key}
              style={{
                display: 'flex',
                flex:
                  STEPS[activeStep].blocks.length > 1 ? '1 1 0' : '0 0 auto',
                minHeight: 0,
              }}
            >
              <AnnotatedBlock block={b} html={blockHtmlByKey[b.key]} />
            </div>
          ))}
          {activeStep === 3 && (
            <div
              style={{
                color: MUTED,
                fontFamily: 'var(--font-serif)',
                fontSize: 14,
                fontStyle: 'italic',
                marginTop: 8,
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
        bottom: -32,
        display: 'flex',
        gap: 8,
        justifyContent: 'center',
        left: 0,
        position: 'absolute',
        right: 0,
      }}
    >
      {STEPS.map((_, i) => {
        const active = i === activeStep;
        return (
          <span
            key={`pill-${i}`}
            style={{
              background: active ? ACCENT : 'transparent',
              border: `1px solid ${active ? ACCENT : BORDER}`,
              borderRadius: 999,
              height: 6,
              transition: 'width 200ms ease, background-color 200ms ease',
              width: active ? 22 : 10,
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
      const next = Math.max(
        0,
        Math.min(text.length, Math.round(v * text.length)),
      );
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
    offset: ['start start', 'end end'],
    target: ref,
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
      aria-label="Incremental BAML adoption"
      ref={ref}
      style={{
        background: BG,
        color: INK,
        padding: '120px 0 160px',
        width: '100%',
      }}
    >
      <div
        style={{
          margin: '0 auto',
          marginBottom: 40,
          maxWidth: 1200,
          padding: '0 32px 0 48px',
        }}
      >
        <p
          style={{
            color: '#8A8580',
            fontSize: 13,
            fontWeight: 500,
            letterSpacing: '0.12em',
            margin: 0,
            textTransform: 'uppercase',
          }}
        >
          Adopt BAML gradually
        </p>
        <h2
          style={{
            fontSize: 'clamp(2rem, 4vw, 3rem)',
            fontWeight: 600,
            letterSpacing: '-0.03em',
            lineHeight: 1.05,
            margin: '12px 0 0',
          }}
        >
          From f-strings to a language, one step at a time.
        </h2>
      </div>

      <div
        style={{
          display: 'grid',
          gap: 48,
          gridTemplateColumns: '40% 60%',
          margin: '0 auto',
          maxWidth: 1200,
          padding: '0 32px 0 48px',
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
                  paddingBottom: 32,
                  paddingLeft: 24,
                }}
              >
                <div
                  style={{
                    background: BG,
                    borderTop: `1px solid ${BORDER}`,
                    paddingBottom: 14,
                    paddingRight: 16,
                    paddingTop: 14,
                    position: 'sticky',
                    top: stickyTop,
                    transition: 'opacity 350ms ease',
                    zIndex: STEPS.length - i,
                  }}
                >
                  <div
                    style={{
                      alignItems: 'baseline',
                      display: 'flex',
                      gap: 16,
                    }}
                  >
                    <div
                      style={{
                        color: isActive ? ACCENT : '#8A8580',
                        flexShrink: 0,
                        fontSize: 11,
                        fontWeight: 500,
                        letterSpacing: '0.14em',
                        textTransform: 'uppercase',
                        transition: 'color 350ms ease',
                      }}
                    >
                      Step {String(i + 1).padStart(2, '0')}
                    </div>
                    <h3
                      style={{
                        color: isActive ? INK : MUTED,
                        fontSize: 'clamp(1.25rem, 2vw, 1.65rem)',
                        fontWeight: 600,
                        letterSpacing: '-0.02em',
                        lineHeight: 1.15,
                        margin: 0,
                        transition: 'color 350ms ease',
                      }}
                    >
                      {s.heading}
                    </h3>
                  </div>
                </div>
                <p
                  style={{
                    color: MUTED,
                    fontSize: 16,
                    lineHeight: 1.6,
                    marginTop: 20,
                    maxWidth: 440,
                  }}
                >
                  <TypingText progress={stepProgresses[i]} text={s.body} />
                </p>
              </section>
            );
          })}
        </div>

        <div
          style={{
            alignSelf: 'start',
            height: 'min(640px, 82vh)',
            position: 'sticky',
            top: 'calc(var(--navigation-height, 56px) + 32px)',
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
