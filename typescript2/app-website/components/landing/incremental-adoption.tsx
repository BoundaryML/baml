'use client';

import {
  AnimatePresence,
  motion,
  useMotionValueEvent,
  useReducedMotion,
  useScroll,
} from 'motion/react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';

// Tokens
const BG = '#F5F1E8';
const BORDER = '#D9D3C4';
const ACCENT = '#7C3AED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
const HAND = 'var(--font-caveat), "Caveat", cursive';

// Code geometry — must match CodeBlock styles below.
const TAB_HEIGHT = 30;
const CODE_PAD_TOP = 12;
const CODE_PAD_LEFT = 16;
const LINE_HEIGHT = 20;
const CHAR_WIDTH = 7.2;
const lineCenterY = (n: number) =>
  TAB_HEIGHT + 1 + CODE_PAD_TOP + (n - 0.5) * LINE_HEIGHT;
const colCenterX = (c: number) => CODE_PAD_LEFT + (c - 1) * CHAR_WIDTH;

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
  const codeRef = useRef<HTMLDivElement>(null);
  const [codeWidth, setCodeWidth] = useState(0);

  useLayoutEffect(() => {
    const el = codeRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      setCodeWidth(entry.contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const GAP = 16;
  const transform = block.scale ? `scale(${block.scale})` : undefined;

  return (
    <div
      style={{
        position: 'relative',
        display: 'grid',
        gridTemplateColumns: '1.6fr 1fr',
        columnGap: GAP,
        transform,
        transformOrigin: 'top left',
      }}
    >
      <div ref={codeRef}>
        <CodeBlock html={html} filename={block.filename} />
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
              fontStyle: 'italic',
              fontSize: 19,
              lineHeight: 1.2,
              color: ACCENT,
              opacity: 0.78,
              whiteSpace: 'pre-line',
              pointerEvents: 'none',
            }}
          >
            {a.text}
          </motion.div>
        ))}
      </div>

      {codeWidth > 0 && (
        <svg
          width="100%"
          height="100%"
          style={{
            position: 'absolute',
            inset: 0,
            pointerEvents: 'none',
            overflow: 'visible',
          }}
        >
          <title>annotation arrows</title>
          {block.annotations.map((a, i) => {
            const tx = colCenterX(a.column);
            const ty = lineCenterY(a.lineNumber);
            const sx = codeWidth + GAP;
            const sy = ty;
            const dx = sx - tx;
            const c1x = sx - dx * 0.35;
            const c1y = sy - 22;
            const c2x = tx + dx * 0.35;
            const c2y = ty - 22;
            const headSize = 5;
            return (
              <g
                key={`${block.key}-arrow-${i}`}
                stroke={ACCENT}
                strokeOpacity={0.55}
                fill="none"
              >
                <motion.path
                  initial={{ pathLength: 0, opacity: 0 }}
                  animate={{ pathLength: 1, opacity: 1 }}
                  transition={{ duration: 0.5, delay: 0.15 + i * 0.08 }}
                  d={`M ${sx} ${sy} C ${c1x} ${c1y}, ${c2x} ${c2y}, ${tx + headSize} ${ty}`}
                  strokeWidth={1.4}
                  strokeLinecap="round"
                />
                <motion.path
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 0.2, delay: 0.6 + i * 0.08 }}
                  d={`M ${tx + headSize + 1} ${ty - 3} L ${tx} ${ty} L ${tx + headSize + 1} ${ty + 3}`}
                  strokeWidth={1.4}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </g>
            );
          })}
        </svg>
      )}
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
            <AnnotatedBlock key={b.key} block={b} html={blockHtmlByKey[b.key]} />
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

// ── Main component ────────────────────────────────────────────────────────────

export function IncrementalAdoption() {
  const ref = useRef<HTMLDivElement>(null);
  const { scrollYProgress } = useScroll({
    target: ref,
    offset: ['start start', 'end end'],
  });
  const [activeStep, setActiveStep] = useState(0);

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
          gridTemplateColumns: '60% 40%',
          gap: 48,
        }}
      >
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

        <div>
          {STEPS.map((s, i) => (
            <section
              key={`step-text-${i}`}
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
