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

const STEP_4_BAML = `class Answer   { text string }
class ReadFile { path string }
class RunBash  { command string }
type Tool = Answer | ReadFile | RunBash

class Action { thought string  tool Tool }

function Agent(message: string) -> Action {
  client GPT4
  prompt #"
    Pick a tool to handle the user request.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ message }}
  "#
}

function dispatch(tool: Tool) -> string {
  match (tool) {
    a: Answer    => a.text,
    r: ReadFile  => baml.fs.read(r.path),
    b: RunBash   => baml.sys.shell(b.command),
  }
}

testset "agent" {
  test "picks the right tool" {
    let act = Agent("read package.json");
    assert.is_true(act.tool is ReadFile);
  }
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
    body: 'Class, prompt, and client live together. The return type drives schema aware parsing. Your Python app calls a typed function. The model never sees a hand rolled JSON schema.',
    heading:
      'Define the function once. Call it from anywhere.',
  },
  {
    blocks: [
      {
        annotations: [
          {
            column: 14,
            lineNumber: 4,
            text: 'each tool is a class.\nadd a tool, add a match arm.',
          },
          {
            column: 3,
            lineNumber: 18,
            text: 'exhaustive match.\nmissing a tool is a compile error.',
          },
          {
            column: 1,
            lineNumber: 25,
            text: 'tests live next to the code.\nrun in the playground or CI.',
          },
        ],
        code: STEP_4_BAML,
        filename: 'agent.baml',
        key: 's4-baml',
        lang: 'baml',
      },
    ],
    body: 'Tagged union tool dispatch via match. Schema aware parsing of model output. testset blocks beside the code. Stdlib written in BAML. The agent loop is BAML.',
    heading: 'We need a whole new language.',
  },
];

// ── Shiki tokenizer hook ─────────────────────────────────────────────────────

type HighlightInput = { code: string; lang: 'python' | 'baml' };
type CodeToken = { content: string; color?: string };
type CodeTokens = CodeToken[][];

const LINE_NUM_WIDTH = 40;
const CODE_BG = '#FDFBF6';
const GUTTER_BG = '#F5F1E5';

function useTokenized(inputs: HighlightInput[]): CodeTokens[] {
  const [out, setOut] = useState<CodeTokens[]>(() =>
    inputs.map((i) =>
      i.code.split('\n').map((line) => [{ content: line }] as CodeToken[]),
    ),
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
        const results: CodeTokens[] = inputs.map((i) => {
          const r = highlighter.codeToTokens(i.code, {
            lang: i.lang as any,
            theme: 'github-light',
          });
          return r.tokens.map((line) =>
            line.map((t) => ({ content: t.content, color: t.color })),
          );
        });
        if (!cancelled) setOut(results);
      } catch {
        /* fall back to plain text — initial state already covers it */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return out;
}

// ── Code block — editor-like surface with line numbers and per-token colors ──

function CodeBlock({
  tokens,
  filename,
}: {
  tokens: CodeTokens;
  filename: string;
}) {
  return (
    <div
      style={{
        background: CODE_BG,
        border: `1px solid ${BORDER}`,
        borderRadius: 8,
        boxShadow:
          '0 1px 0 rgba(0,0,0,0.02), 0 8px 28px -18px rgba(0,0,0,0.22)',
        overflow: 'hidden',
        width: '100%',
      }}
    >
      {/* Window chrome: traffic lights + filename tab */}
      <div
        style={{
          alignItems: 'center',
          background: GUTTER_BG,
          borderBottom: `1px solid ${BORDER}`,
          boxSizing: 'border-box',
          color: MUTED,
          display: 'grid',
          gridTemplateColumns: '60px 1fr 60px',
          height: TAB_HEIGHT,
          padding: '0 12px',
        }}
      >
        <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
          <span
            style={{
              background: '#E5A8A8',
              border: '1px solid rgba(0,0,0,0.06)',
              borderRadius: '50%',
              height: 10,
              width: 10,
            }}
          />
          <span
            style={{
              background: '#E5C58A',
              border: '1px solid rgba(0,0,0,0.06)',
              borderRadius: '50%',
              height: 10,
              width: 10,
            }}
          />
          <span
            style={{
              background: '#A8D0A0',
              border: '1px solid rgba(0,0,0,0.06)',
              borderRadius: '50%',
              height: 10,
              width: 10,
            }}
          />
        </div>
        <span
          style={{
            fontFamily: MONO,
            fontSize: 11,
            letterSpacing: '0.04em',
            color: MUTED,
            textAlign: 'center',
          }}
        >
          {filename}
        </span>
        <span />
      </div>

      {/* Editor body: line numbers gutter + code */}
      <div
        className="adoption-code"
        style={{
          display: 'flex',
          fontFamily: MONO,
          fontSize: 13.5,
          lineHeight: `${LINE_HEIGHT}px`,
          background: CODE_BG,
        }}
      >
        <div
          aria-hidden
          style={{
            width: LINE_NUM_WIDTH,
            flexShrink: 0,
            background: GUTTER_BG,
            borderRight: `1px solid ${BORDER}`,
            color: '#B8B0A0',
            textAlign: 'right',
            padding: `${CODE_PAD_TOP}px 8px`,
            userSelect: 'none',
            fontVariantNumeric: 'tabular-nums',
          }}
        >
          {tokens.map((_, i) => (
            <div key={`ln-${i}`} style={{ height: LINE_HEIGHT }}>
              {i + 1}
            </div>
          ))}
        </div>
        <pre
          style={{
            margin: 0,
            padding: `${CODE_PAD_TOP}px ${CODE_PAD_LEFT}px`,
            flex: 1,
            minWidth: 0,
            color: INK,
            background: 'transparent',
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
            overflowWrap: 'anywhere',
            tabSize: 4,
          }}
        >
          <code style={{ fontFamily: MONO, background: 'transparent' }}>
            {tokens.map((line, i) => (
              <div
                key={`l-${i}`}
                style={{ minHeight: LINE_HEIGHT }}
              >
                {line.length === 0 ? (
                  <span>&#8203;</span>
                ) : (
                  line.map((tok, j) => (
                    <span
                      key={`t-${i}-${j}`}
                      style={{ color: tok.color || INK }}
                    >
                      {tok.content}
                    </span>
                  ))
                )}
              </div>
            ))}
          </code>
        </pre>
      </div>
    </div>
  );
}

// ── Annotated block: code on the left, caption gutter on the right ───────────

function AnnotatedBlock({
  tokens,
  block,
}: {
  tokens: CodeTokens;
  block: BlockSpec;
}) {
  const GAP = 16;
  const transform = block.scale ? `scale(${block.scale})` : undefined;

  return (
    <div
      style={{
        columnGap: GAP,
        display: 'grid',
        gridTemplateColumns: '3.4fr 1fr',
        position: 'relative',
        transform,
        transformOrigin: 'top left',
        width: '100%',
      }}
    >
      <div style={{ minWidth: 0, position: 'relative', width: '100%' }}>
        <CodeBlock filename={block.filename} tokens={tokens} />
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
  const [tok1, tok2, tokBaml3, tokPy3, tokBaml4] = useTokenized([
    { code: STEP_1_PY, lang: 'python' },
    { code: STEP_2_PY, lang: 'python' },
    { code: STEP_3_BAML, lang: 'baml' },
    { code: STEP_3_PY, lang: 'python' },
    { code: STEP_4_BAML, lang: 'baml' },
  ]);

  const fadeT = reduced ? { duration: 0 } : { duration: 0.35 };

  const blockTokensByKey: Record<string, CodeTokens> = {
    's1-py': tok1,
    's2-py': tok2,
    's3-baml': tokBaml3,
    's3-py': tokPy3,
    's4-baml': tokBaml4,
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
                flex: '0 0 auto',
                minHeight: 0,
                width: '100%',
              }}
            >
              <AnnotatedBlock block={b} tokens={blockTokensByKey[b.key]} />
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
  // Type-in/out windows are short; the bulk of each step's scroll budget
  // is the fully-typed "hold" so readers actually have time to read.
  const step0Progress = useTransform(
    scrollYProgress,
    [0, 0.001, 0.18, 0.22],
    [1, 1, 1, 0],
  );
  const step1Progress = useTransform(
    scrollYProgress,
    [0.22, 0.26, 0.44, 0.48],
    [0, 1, 1, 0],
  );
  const step2Progress = useTransform(
    scrollYProgress,
    [0.48, 0.52, 0.70, 0.74],
    [0, 1, 1, 0],
  );
  const step3Progress = useTransform(
    scrollYProgress,
    [0.74, 0.78, 1],
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
          maxWidth: 1360,
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
          gap: 40,
          gridTemplateColumns: '28% 72%',
          margin: '0 auto',
          maxWidth: 1360,
          padding: '0 24px 0 32px',
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
                  <motion.div
                    initial={false}
                    animate={{
                      maxHeight: isActive ? 320 : 0,
                      opacity: isActive ? 1 : 0,
                      marginTop: isActive ? 20 : 0,
                    }}
                    transition={{
                      maxHeight: { duration: 0.5, ease: [0.22, 0.61, 0.36, 1] },
                      opacity: {
                        duration: 0.3,
                        ease: 'easeInOut',
                        delay: isActive ? 0.1 : 0,
                      },
                      marginTop: { duration: 0.5, ease: [0.22, 0.61, 0.36, 1] },
                    }}
                    style={{
                      overflow: 'hidden',
                      fontSize: 16,
                      lineHeight: 1.6,
                      color: MUTED,
                      maxWidth: 440,
                    }}
                  >
                    <p style={{ margin: 0 }}>
                      <TypingText text={s.body} progress={stepProgresses[i]} />
                    </p>
                  </motion.div>
                </div>
              </section>
            );
          })}
        </div>

        <div
          style={{
            alignSelf: 'start',
            height: 'min(760px, 88vh)',
            overflow: 'hidden',
            position: 'sticky',
            top: 'calc(var(--navigation-height, 56px) + 32px)',
          }}
        >
          <StickyPanel activeStep={activeStep} />
        </div>
      </div>

    </section>
  );
}
