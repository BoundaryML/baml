'use client';

import {
  AnimatePresence,
  motion,
  useMotionValue,
  useMotionValueEvent,
  useReducedMotion,
  useScroll,
} from 'motion/react';
import { useEffect, useRef, useState } from 'react';
import { useIsMobile } from '@/hooks/use-media-query';

type ScrollYProgress = ReturnType<typeof useScroll>['scrollYProgress'];

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
const SECTION_STICKY_TOP = '130px';

// Step thresholds for scroll progress. Must match the thresholds in
// `useMotionValueEvent` inside `IncrementalAdoption` below.
const STEP_RANGES: ReadonlyArray<[number, number]> = [
  [0, 0.22],
  [0.22, 0.48],
  [0.48, 0.74],
  [0.74, 1],
];

// ── Code sources ─────────────────────────────────────────────────────────────

const STEP_1_PY = `from openai import OpenAI
from pydantic import BaseModel

client = OpenAI()

class LineItem(BaseModel):
    name: str
    qty: int
    price: float

class Invoice(BaseModel):
    vendor: str
    total: float
    due_date: str
    line_items: list[LineItem]

def extract_invoice(text: str) -> Invoice | None:
    response = client.chat.completions.parse(
        model="gpt-4o",
        messages=[{
            "role": "user",
            "content": f"Extract the invoice from:\\n\\n{text}",
        }],
        response_format=Invoice,
    )
    return response.choices[0].message.parsed
`;

const STEP_2_BAML = `class LineItem { name string  qty int  price float }

class Invoice {
  vendor      string
  total       float
  due_date    string
  line_items  LineItem[]
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

invoice = b.ExtractInvoice(raw_pdf_text)

invoice.
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
    heading: 'Spaghetti code and strings everywhere.',
    body: 'Pydantic schemas, OpenAI SDK structured outputs. The prompt still lives as a string in your app — untyped, untested, Python-only.',
    blocks: [
      {
        key: 's1-py',
        code: STEP_1_PY,
        lang: 'python',
        filename: 'main.py',
        annotations: [
          {
            lineNumber: 22,
            text: 'the prompt is still a string\nin your app code.',
          },
          {
            lineNumber: 24,
            text: 'schema lives in Python.\ncan’t be reused elsewhere.',
          },
        ],
      },
    ],
  },
  {
    marker: 'Step 02. Lift one prompt out.',
    heading: 'Turn your LLM calls into typed BAML functions.',
    body: 'Schema, prompt, and SDK live in BAML — but the rest of your app stays in Python.',
    blocks: [
      {
        key: 's2-baml',
        code: STEP_2_BAML,
        lang: 'baml',
        filename: 'invoice.baml',
        annotations: [
          {
            lineNumber: 10,
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
            lineNumber: 3,
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
            lineNumber: 13,
            text: 'real for loop.\ntyped maps. typed values.',
          },
        ],
      },
    ],
  },
  {
    marker: 'Step 04. The whole loop.',
    heading: 'Orchestration in BAML itself.',
    body: 'Typed io, fs, shell, and http built in — no requests, no asyncio, no JSON plumbing. Faster than Python and safe by default.',
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
            lineNumber: 27,
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

// ── LSP autocomplete popover ─────────────────────────────────────────────────

const COMPLETION_ITEMS: { name: string; type: string }[] = [
  { name: 'vendor', type: 'string' },
  { name: 'total', type: 'float' },
  { name: 'due_date', type: 'string' },
  { name: 'line_items', type: 'LineItem[]' },
];

function AutocompletePopover() {
  // Anchor: line 5 (`invoice.`), one char past the dot.
  const top = lineCenterY(5) + LINE_HEIGHT / 2 + 4;
  const charPx = 8.1;
  const left = LINE_NUM_WIDTH + CODE_PAD_LEFT + 8 * charPx;

  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      initial={{ opacity: 0, y: -4 }}
      role="listbox"
      style={{
        background: '#FFFFFF',
        border: '1px solid #D8D2C4',
        borderRadius: 6,
        boxShadow:
          '0 1px 0 rgba(0,0,0,0.04), 0 12px 28px -8px rgba(26,22,18,0.22)',
        fontFamily: MONO,
        fontSize: 12,
        left,
        minWidth: 240,
        overflow: 'hidden',
        position: 'absolute',
        top,
        zIndex: 5,
      }}
      transition={{ delay: 0.35, duration: 0.35 }}
    >
      {COMPLETION_ITEMS.map((item, i) => (
        <div
          key={item.name}
          style={{
            alignItems: 'center',
            background: i === 0 ? 'rgba(124,58,237,0.10)' : 'transparent',
            color: INK,
            display: 'flex',
            gap: 8,
            padding: '6px 10px',
          }}
        >
          <span
            aria-hidden
            style={{
              alignItems: 'center',
              background: '#FBF1D7',
              border: '1px solid #E8CB8C',
              borderRadius: 3,
              color: '#7A5A12',
              display: 'flex',
              flexShrink: 0,
              fontFamily: MONO,
              fontSize: 9,
              fontWeight: 700,
              height: 14,
              justifyContent: 'center',
              lineHeight: 1,
              width: 14,
            }}
          >
            f
          </span>
          <span style={{ color: INK, fontWeight: 500 }}>{item.name}</span>
          <span
            style={{
              color: MUTED,
              flex: 1,
              fontSize: 11,
              textAlign: 'right',
            }}
          >
            {item.type}
          </span>
        </div>
      ))}
    </motion.div>
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
        {block.key === 's2-py' && <AutocompletePopover />}
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

function MigrationProgress({ activeStep }: { activeStep: number }) {
  const SIZE = 60;
  const STROKE = 4;
  const RADIUS = (SIZE - STROKE) / 2;
  const CIRC = 2 * Math.PI * RADIUS;
  const pct = Math.round((activeStep / (STEPS.length - 1)) * 100);
  const dashOffset = CIRC * (1 - pct / 100);

  const showLamb = activeStep >= 2;
  const isComplete = activeStep >= 3;

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
        <motion.div
          animate={{
            boxShadow: isComplete
              ? '0 0 0 2px rgba(124,58,237,0.45), 0 0 24px rgba(124,58,237,0.55), 0 0 48px rgba(124,58,237,0.35)'
              : '0 0 0 0 rgba(124,58,237,0)',
          }}
          style={{
            alignItems: 'center',
            background: BG,
            borderRadius: '50%',
            display: 'grid',
            inset: STROKE + 2,
            placeItems: 'center',
            position: 'absolute',
          }}
          transition={{ duration: 0.5, ease: [0.22, 0.61, 0.36, 1] }}
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
        </motion.div>
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

function StickyPanel({
  activeStep,
  scrollYProgress,
}: {
  activeStep: number;
  scrollYProgress: ScrollYProgress;
}) {
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

  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const y = useMotionValue(0);
  const [edgeFade, setEdgeFade] = useState<{ top: number; bottom: number }>({
    top: 0,
    bottom: 0,
  });

  // Drives a translateY on the inner content as the user scrolls within
  // the current step's progress range — so tall snippets reveal their
  // bottom without ever capturing wheel events from the page.
  const applyPan = (latest: number) => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (!container || !content) return;
    const overflow = Math.max(0, content.scrollHeight - container.clientHeight);
    if (overflow === 0) {
      y.set(0);
      setEdgeFade({ top: 0, bottom: 0 });
      return;
    }
    const [start, end] = STEP_RANGES[activeStep] ?? [0, 1];
    const within = Math.max(0, Math.min(1, (latest - start) / (end - start)));
    y.set(-overflow * within);
    // Fade in/out the top and bottom edge masks based on how much can be
    // panned in each direction (ramps in over the first 24px of pan).
    setEdgeFade({
      top: Math.min(1, (overflow * within) / 24),
      bottom: Math.min(1, (overflow * (1 - within)) / 24),
    });
  };

  useMotionValueEvent(scrollYProgress, 'change', applyPan);

  // Recompute pan when the active step (and therefore content height) changes.
  useEffect(() => {
    applyPan(scrollYProgress.get());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeStep, tokens]);

  const FADE = 64;

  return (
    <div
      className="step-code-scroll"
      ref={containerRef}
      style={{
        height: '100%',
        minHeight: 520,
        overflow: 'hidden',
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
          ref={contentRef}
          style={{
            display: 'flex',
            flexDirection: 'column',
            gap: 12,
            minHeight: '100%',
            position: 'relative',
            y,
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
      <div
        aria-hidden
        style={{
          background: `linear-gradient(to bottom, ${BG}, rgba(255,255,255,0))`,
          height: FADE,
          left: 0,
          opacity: edgeFade.top,
          pointerEvents: 'none',
          position: 'absolute',
          right: 8,
          top: 0,
          transition: 'opacity 150ms ease-out',
        }}
      />
      <div
        aria-hidden
        style={{
          background: `linear-gradient(to top, ${BG}, rgba(255,255,255,0))`,
          bottom: 0,
          height: FADE,
          left: 0,
          opacity: edgeFade.bottom,
          pointerEvents: 'none',
          position: 'absolute',
          right: 8,
          transition: 'opacity 150ms ease-out',
        }}
      />
    </div>
  );
}

// ── Mobile: simplified static stack (no scroll choreography) ──────────────────

function MobileAdoption() {
  const tokens = useTokenized([
    { code: STEP_1_PY, lang: 'python' },
    { code: STEP_2_BAML, lang: 'baml' },
    { code: STEP_2_PY, lang: 'python' },
    { code: STEP_3_BAML, lang: 'baml' },
    { code: STEP_4_BAML, lang: 'baml' },
  ]);
  const tokensByKey: Record<string, CodeTokens> = {
    's1-py': tokens[0],
    's2-baml': tokens[1],
    's2-py': tokens[2],
    's3-baml': tokens[3],
    's4-baml': tokens[4],
  };

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 48,
        margin: '0 auto',
        maxWidth: 520,
        padding: '0 20px',
      }}
    >
      <h2
        style={{
          color: INK,
          fontSize: 'clamp(2rem, 9vw, 2.6rem)',
          fontWeight: 600,
          letterSpacing: '-0.03em',
          lineHeight: 1.0,
          margin: 0,
        }}
      >
        Adopt BAML Incrementally
      </h2>
      {STEPS.map((s, i) => (
        <div key={`m-step-${i}`}>
          <div
            style={{
              color: ACCENT,
              fontSize: 11,
              fontWeight: 600,
              letterSpacing: '0.14em',
              textTransform: 'uppercase',
            }}
          >
            {s.marker}
          </div>
          <h3
            style={{
              color: INK,
              fontSize: '1.3rem',
              fontWeight: 600,
              letterSpacing: '-0.02em',
              lineHeight: 1.15,
              margin: '12px 0 0',
            }}
          >
            {s.heading}
          </h3>
          <p
            style={{
              color: MUTED,
              fontSize: 15,
              lineHeight: 1.6,
              margin: '12px 0 18px',
            }}
          >
            {s.body}
          </p>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            {s.blocks.map((b) => (
              <CodeBlock
                filename={b.filename}
                key={b.key}
                tokens={tokensByKey[b.key]}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function IncrementalAdoption() {
  const ref = useRef<HTMLDivElement>(null);
  const isMobile = useIsMobile();
  const { scrollYProgress } = useScroll({
    offset: ['start start', 'end end'],
    target: ref,
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

  if (isMobile) {
    return (
      <section
        aria-label="Incremental BAML adoption"
        style={{
          background: BG,
          color: INK,
          padding: '72px 0 96px',
          width: '100%',
        }}
      >
        <MobileAdoption />
      </section>
    );
  }

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
        className="adoption-grid"
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
            Adopt BAML Incrementally
          </h2>
          {STEPS.map((s, i) => {
            const isActive = i === activeStep;
            const STACK_TOP_BASE = 160;
            const STACK_TAB_HEIGHT = 56;
            const stickyTop = STACK_TOP_BASE + i * STACK_TAB_HEIGHT;
            return (
              <section
                className="adoption-step"
                key={`step-text-${i}`}
                style={{
                  minHeight: '120vh',
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
                    <p style={{ margin: 0 }}>{s.body}</p>
                  </motion.div>
                </div>
              </section>
            );
          })}
        </div>

        <div
          className="adoption-visual"
          style={{
            alignSelf: 'start',
            display: 'flex',
            flexDirection: 'column',
            gap: 16,
            height: `min(840px, calc(100vh - ${SECTION_STICKY_TOP} - 32px))`,
            minHeight: 520,
            overflow: 'hidden',
            position: 'sticky',
            top: SECTION_STICKY_TOP,
          }}
        >
          <div
            style={{
              background: BG,
              position: 'sticky',
              top: 0,
              zIndex: 2,
            }}
          >
            <MigrationProgress activeStep={activeStep} />
          </div>
          <div style={{ flex: 1, minHeight: 0, overflow: 'hidden' }}>
            <StickyPanel
              activeStep={activeStep}
              scrollYProgress={scrollYProgress}
            />
          </div>
        </div>
      </div>
    </section>
  );
}
