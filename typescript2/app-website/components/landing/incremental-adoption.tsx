'use client';

import {
  AnimatePresence,
  type MotionValue,
  animate,
  motion,
  useMotionValue,
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

// Code geometry (must match CodeBlock styles below).
const TAB_HEIGHT = 30;
const CODE_PAD_TOP = 12;
const CODE_PAD_LEFT = 16;
const LINE_HEIGHT = 20;
const lineCenterY = (n: number) =>
  TAB_HEIGHT + 1 + CODE_PAD_TOP + (n - 0.5) * LINE_HEIGHT;

// ── Code sources ─────────────────────────────────────────────────────────────

const STEP_1_PY = `from openai import OpenAI
import json

client = OpenAI()

def extract_invoice(text: str):
    response = client.chat.completions.create(
        model="gpt-4o",
        messages=[{
            "role": "user",
            "content": f"""Extract invoice fields. Return JSON with
vendor, total (float), due_date (YYYY-MM-DD),
line_items (array of name, qty, price).

Text: {text}"""
        }]
    )
    try:
        return json.loads(response.choices[0].message.content)
    except json.JSONDecodeError:
        return None  # silent failure
`;

const STEP_2_BAML = `class LineItem {
  name  string
  qty   int
  price float
}

class Invoice {
  vendor     string
  total      float
  due_date   string
  line_items LineItem[]
}

function ExtractInvoice(text: string) -> Invoice {
  client GPT4o
  prompt #"
    Extract invoice fields from the text.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ text }}
  "#
}
`;

const STEP_2_PY = `from baml_client import b

# typed result. your python app stays in python.
invoice = b.ExtractInvoice(raw_pdf_text)
print(invoice.total)               # typed float
print(invoice.line_items[0].name)  # typed string
`;

const STEP_3_BAML = `// pure BAML helpers. no LLM. typed. fast.
function bucket(invoice: Invoice) -> "small" | "medium" | "large" {
  if (invoice.total > 10000.0) { "large" }
  else if (invoice.total > 1000.0) { "medium" }
  else { "small" }
}

function summarize(invoices: Invoice[]) -> map<string, int> {
  let counts: map<string, int> = {};
  counts.set("small", 0);
  counts.set("medium", 0);
  counts.set("large", 0);
  for (let inv in invoices) {
    let key = bucket(inv);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  counts
}
`;

const STEP_4_BAML = `class Answer   { text string }
class ReadFile { path string }
class RunBash  { command string }
type Tool = Answer | ReadFile | RunBash

class Step { thought string  tool Tool }

function PickTool(history: string[]) -> Step {
  client GPT4o
  prompt #"
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ history }}
  "#
}

// exhaustive at compile time.
// adding a Tool variant without updating this is a type error.
function dispatch(tool: Tool) -> string {
  match (tool) {
    a: Answer    => a.text,
    r: ReadFile  => baml.fs.read(r.path),
    b: RunBash   => baml.sys.shell(b.command).stdout,
  }
}

// the agent loop, in BAML itself. typed history, typed tools.
function main() -> string {
  let history: string[] = [baml.io.input(">> ")];
  for (let _ = 0; _ < 5; _ += 1) {
    let step = PickTool(history);
    let result = dispatch(step.tool);
    if (step.tool is Answer) { return result; };
    history.push(result);
  }
  "(agent hit turn limit)"
}

testset "agent" {
  test "answers route through dispatch" {
    assert.equal(dispatch(Answer { text: "ok" }), "ok");
  }
}
`;

type Annotation = {
  text: string;
  lineNumber: number;
};

type BlockSpec = {
  key: string;
  code: string;
  lang: 'python' | 'baml';
  filename: string;
  annotations: Annotation[];
};

type Step = {
  marker: string;
  heading: string;
  body: string;
  blocks: BlockSpec[];
};

const STEPS: Step[] = [
  {
    marker: 'Step 01. Today.',
    heading: 'Strings, json.loads, fingers crossed.',
    body: 'A string prompt. Manual JSON parsing. No types. Keep this file open — the rest of the timeline lifts pieces out of it, one at a time.',
    blocks: [
      {
        key: 's1-py',
        code: STEP_1_PY,
        lang: 'python',
        filename: 'main.py',
        annotations: [
          {
            lineNumber: 11,
            text: 'string prompt.\nJSON.loads and hope.',
          },
          {
            lineNumber: 22,
            text: 'silent failure when\nJSON is malformed.',
          },
        ],
      },
    ],
  },
  {
    marker: 'Step 02. Lift one prompt out.',
    heading: 'The first typed BAML function.',
    body: 'Schema, prompt, and client live once in BAML. Your Python keeps running. The only thing that changes is how that one call looks — and that the result is typed.',
    blocks: [
      {
        key: 's2-baml',
        code: STEP_2_BAML,
        lang: 'baml',
        filename: 'invoice.baml',
        annotations: [
          {
            lineNumber: 14,
            text: 'BAML injects the schema.\nthe model knows what to return.',
          },
        ],
      },
      {
        key: 's2-py',
        code: STEP_2_PY,
        lang: 'python',
        filename: 'main.py',
        annotations: [
          {
            lineNumber: 4,
            text: 'fully typed in your app.\nIDE knows the shape.',
          },
        ],
      },
    ],
  },
  {
    marker: 'Step 03. Move the helpers next.',
    heading: 'Pure logic, no LLM, typed and fast.',
    body: 'Bucket and summarize stop being Python helpers. They become BAML functions running on the BAML VM — exhaustive, typed, faster than Python on object-heavy workloads.',
    blocks: [
      {
        key: 's3-baml',
        code: STEP_3_BAML,
        lang: 'baml',
        filename: 'reports.baml',
        annotations: [
          {
            lineNumber: 2,
            text: 'literal union return type.\nthe compiler enforces the set.',
          },
          {
            lineNumber: 8,
            text: 'real for loop.\ntyped maps. typed values.',
          },
        ],
      },
    ],
  },
  {
    marker: 'Step 04. The whole loop.',
    heading: 'Orchestration in BAML itself.',
    body: 'Tagged unions, exhaustive match, stdlib for io, fs, shell, http. Tests next to the code. The agent loop is now BAML — Python is gone.',
    blocks: [
      {
        key: 's4-baml',
        code: STEP_4_BAML,
        lang: 'baml',
        filename: 'agent.baml',
        annotations: [
          {
            lineNumber: 4,
            text: 'tagged union. add a tool,\nadd a match arm.',
          },
          {
            lineNumber: 19,
            text: 'exhaustive match.\nmissing a variant is a compile error.',
          },
          {
            lineNumber: 28,
            text: 'main() runs on the BAML VM.\nstdlib for io, fs, shell.',
          },
          {
            lineNumber: 39,
            text: 'tests live next to the code.\nassertions, not snapshots.',
          },
        ],
      },
    ],
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
        /* fall back to plain text */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return out;
}

// ── Code block ───────────────────────────────────────────────────────────────

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
              <div key={`l-${i}`} style={{ minHeight: LINE_HEIGHT }}>
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

// ── Annotated block ──────────────────────────────────────────────────────────

function AnnotatedBlock({
  tokens,
  block,
}: {
  tokens: CodeTokens;
  block: BlockSpec;
}) {
  const GAP = 16;

  return (
    <div
      style={{
        columnGap: GAP,
        display: 'grid',
        gridTemplateColumns: '3.4fr 1fr',
        position: 'relative',
        width: '100%',
      }}
    >
      <div style={{ minWidth: 0, position: 'relative', width: '100%' }}>
        <CodeBlock filename={block.filename} tokens={tokens} />
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

// ── Migration progress artifact ──────────────────────────────────────────────

function MigrationProgress({
  activeStep,
}: {
  activeStep: number;
}) {
  const SIZE = 60;
  const STROKE = 4;
  const RADIUS = (SIZE - STROKE) / 2;
  const CIRC = 2 * Math.PI * RADIUS;
  const pct = Math.round((activeStep / (STEPS.length - 1)) * 100);
  const dashOffset = CIRC * (1 - pct / 100);

  const showLamb = activeStep >= 3;

  return (
    <div
      style={{
        alignItems: 'center',
        background: '#FBF8F1',
        border: `1px solid ${BORDER}`,
        borderRadius: 10,
        display: 'flex',
        gap: 16,
        padding: '14px 18px',
      }}
    >
      <div
        style={{
          flexShrink: 0,
          height: SIZE,
          position: 'relative',
          width: SIZE,
        }}
      >
        <svg
          aria-hidden
          height={SIZE}
          style={{
            inset: 0,
            position: 'absolute',
            transform: 'rotate(-90deg)',
          }}
          width={SIZE}
        >
          <circle
            cx={SIZE / 2}
            cy={SIZE / 2}
            fill="none"
            r={RADIUS}
            stroke="#EBE5D5"
            strokeWidth={STROKE}
          />
          <motion.circle
            animate={{ strokeDashoffset: dashOffset }}
            cx={SIZE / 2}
            cy={SIZE / 2}
            fill="none"
            r={RADIUS}
            stroke={ACCENT}
            strokeDasharray={CIRC}
            strokeLinecap="round"
            strokeWidth={STROKE}
            initial={false}
            transition={{ duration: 0.32, ease: [0.22, 0.61, 0.36, 1] }}
          />
        </svg>
        <div
          style={{
            alignItems: 'center',
            background: BG,
            borderRadius: '50%',
            display: 'grid',
            inset: STROKE + 2,
            placeItems: 'center',
            position: 'absolute',
          }}
        >
          <AnimatePresence initial={false} mode="wait">
            {showLamb ? (
              <motion.img
                alt="BAML"
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.7 }}
                height={36}
                initial={{ opacity: 0, scale: 0.7 }}
                key="lamb"
                src="/bamllogopurple.svg"
                style={{ display: 'block', objectFit: 'contain' }}
                transition={{ duration: 0.32, ease: [0.22, 0.61, 0.36, 1] }}
                width={36}
              />
            ) : (
              <motion.img
                alt="Python"
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.7 }}
                height={30}
                initial={{ opacity: 0, scale: 0.7 }}
                key="python"
                src="/python-icon.png"
                style={{ display: 'block', objectFit: 'contain' }}
                transition={{ duration: 0.32, ease: [0.22, 0.61, 0.36, 1] }}
                width={30}
              />
            )}
          </AnimatePresence>
        </div>
      </div>

      <div
        style={{
          display: 'flex',
          flex: 1,
          flexDirection: 'column',
          gap: 6,
          minWidth: 0,
        }}
      >
        <div
          style={{
            color: '#8A8580',
            fontFamily: MONO,
            fontSize: 10.5,
            fontWeight: 600,
            letterSpacing: '0.16em',
            textTransform: 'uppercase',
          }}
        >
          Migration progress
        </div>
        <div
          style={{
            alignItems: 'baseline',
            display: 'flex',
            gap: 8,
          }}
        >
          <span
            style={{
              color: INK,
              fontFamily: MONO,
              fontSize: 18,
              fontVariantNumeric: 'tabular-nums',
              fontWeight: 600,
              letterSpacing: '-0.02em',
            }}
          >
            {pct}%
          </span>
          <span
            style={{
              color: MUTED,
              fontFamily: MONO,
              fontSize: 12,
              letterSpacing: '0.04em',
            }}
          >
            BAML
          </span>
        </div>
        <div
          aria-hidden
          style={{
            alignItems: 'center',
            display: 'flex',
            gap: 6,
            marginTop: 2,
          }}
        >
          {[0, 1, 2, 3].map((i) => (
            <div
              key={`dot-${i}`}
              style={{
                background: i <= activeStep ? ACCENT : 'transparent',
                border: `1px solid ${i <= activeStep ? ACCENT : '#C9C2B0'}`,
                borderRadius: '50%',
                height: 7,
                transition:
                  'background-color 280ms ease, border-color 280ms ease',
                width: 7,
              }}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

// ── Sticky code panel with per-step transitions ──────────────────────────────

function StickyPanel({ activeStep }: { activeStep: number }) {
  const reduced = useReducedMotion();
  const tokens = useTokenized([
    { code: STEP_1_PY, lang: 'python' },
    { code: STEP_2_BAML, lang: 'baml' },
    { code: STEP_2_PY, lang: 'python' },
    { code: STEP_3_BAML, lang: 'baml' },
    { code: STEP_4_BAML, lang: 'baml' },
  ]);

  const fadeT = reduced ? { duration: 0 } : { duration: 0.35 };

  const blockTokensByKey: Record<string, CodeTokens> = {
    's1-py': tokens[0],
    's2-baml': tokens[1],
    's2-py': tokens[2],
    's3-baml': tokens[3],
    's4-baml': tokens[4],
  };

  return (
    <div
      className="step-code-scroll"
      style={{
        height: '100%',
        minHeight: 520,
        overflowY: 'auto',
        paddingRight: 8,
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
            minHeight: '100%',
            position: 'relative',
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
        </motion.div>
      </AnimatePresence>
    </div>
  );
}

// ── Per step body text streamer ──────────────────────────────────────────────

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

  // Per step body typewriter progress.
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
    [0.48, 0.52, 0.7, 0.74],
    [0, 1, 1, 0],
  );
  // Step 4 is the last section, so the user often stops scrolling once they
  // arrive. A scroll-driven typewriter would stall partway. Drive it on a
  // timer instead, kicking off when the step becomes active.
  const step3Progress = useMotionValue(0);
  const reducedMotion = useReducedMotion();

  useEffect(() => {
    if (activeStep !== 3) {
      step3Progress.set(0);
      return;
    }
    if (reducedMotion) {
      step3Progress.set(1);
      return;
    }
    const controls = animate(step3Progress, 1, {
      delay: 0.45,
      duration: 1.6,
      ease: [0.22, 0.61, 0.36, 1],
    });
    return () => controls.stop();
  }, [activeStep, reducedMotion, step3Progress]);

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
          display: 'grid',
          gap: 40,
          gridTemplateColumns: '28% 72%',
          margin: '0 auto',
          maxWidth: 1360,
          padding: '0 24px 0 32px',
        }}
      >
        <div>
          <h2
            style={{
              color: INK,
              fontSize: 'clamp(2rem, 3.2vw, 3.4rem)',
              fontWeight: 600,
              letterSpacing: '-0.03em',
              lineHeight: 0.98,
              margin: '0 0 40px 24px',
            }}
          >
            Adopt Incrementally
          </h2>
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
                      color: isActive ? ACCENT : '#8A8580',
                      display: 'flex',
                      flexShrink: 0,
                      fontSize: 11,
                      fontWeight: 600,
                      letterSpacing: '0.14em',
                      textTransform: 'uppercase',
                      transition: 'color 350ms ease',
                    }}
                  >
                    <span>{s.marker}</span>
                  </div>
                  <h3
                    style={{
                      color: isActive ? INK : MUTED,
                      fontSize: 'clamp(1.25rem, 2vw, 1.65rem)',
                      fontWeight: 600,
                      letterSpacing: '-0.02em',
                      lineHeight: 1.15,
                      margin: '12px 0 0',
                      transition: 'color 350ms ease',
                    }}
                  >
                    {s.heading}
                  </h3>
                  <motion.div
                    initial={false}
                    animate={{
                      maxHeight: isActive ? 320 : 0,
                      opacity: isActive ? 1 : 0,
                      marginTop: isActive ? 20 : 0,
                    }}
                    transition={{
                      maxHeight: {
                        duration: 0.5,
                        ease: [0.22, 0.61, 0.36, 1],
                      },
                      opacity: {
                        duration: 0.3,
                        ease: 'easeInOut',
                        delay: isActive ? 0.1 : 0,
                      },
                      marginTop: {
                        duration: 0.5,
                        ease: [0.22, 0.61, 0.36, 1],
                      },
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
            display: 'flex',
            flexDirection: 'column',
            gap: 16,
            height: 'min(840px, 92vh)',
            overflow: 'hidden',
            position: 'sticky',
            top: 'calc(var(--navigation-height, 56px) + 24px)',
          }}
        >
          <MigrationProgress activeStep={activeStep} />
          <div style={{ flex: 1, minHeight: 0, overflow: 'hidden' }}>
            <StickyPanel activeStep={activeStep} />
          </div>
        </div>
      </div>
    </section>
  );
}
