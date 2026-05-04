'use client';

import { AnimatePresence, motion, useReducedMotion } from 'motion/react';
import { useEffect, useState } from 'react';

const BG = '#ffffff';
const BORDER = '#D9D3C4';
const ACCENT = '#7C3AED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
const HAND = '"Helvetica Neue", Helvetica, Arial, sans-serif';

const TAB_HEIGHT = 30;
const CODE_PAD_TOP = 12;
const CODE_PAD_LEFT = 16;
const LINE_HEIGHT = 20;
const LINE_NUM_WIDTH = 36;
const CODE_BG = '#FDFBF6';
const GUTTER_BG = '#F5F1E5';

// ── Code samples (one comparison per language) ───────────────────────────────

const PY_NATIVE = `from openai import OpenAI
import json

client = OpenAI()

def extract_user(text: str):
    response = client.chat.completions.create(
        model="gpt-4o",
        messages=[{
            "role": "user",
            "content": f"""Extract the user. Return JSON with
name (string), email (string), age (int), tier
(one of free, pro, enterprise).

Text: {text}"""
        }]
    )
    try:
        return json.loads(response.choices[0].message.content)
    except json.JSONDecodeError:
        return None
`;

const TS_NATIVE = `import OpenAI from "openai";

const client = new OpenAI();

async function extractUser(text: string) {
  const r = await client.chat.completions.create({
    model: "gpt-4o",
    messages: [{
      role: "user",
      content: \`Extract the user. Return JSON with
name (string), email (string), age (int), tier
(one of free, pro, enterprise).

Text: \${text}\`,
    }],
  });
  try {
    return JSON.parse(r.choices[0].message.content ?? "");
  } catch {
    return null;
  }
}
`;

const GO_NATIVE = `package main

import (
  "context"
  "encoding/json"
  "fmt"

  "github.com/sashabaranov/go-openai"
)

type User struct {
  Name  string \`json:"name"\`
  Email string \`json:"email"\`
  Age   int    \`json:"age"\`
  Tier  string \`json:"tier"\`
}

func ExtractUser(text string) (*User, error) {
  c := openai.NewClient("OPENAI_API_KEY")
  resp, err := c.CreateChatCompletion(context.Background(), openai.ChatCompletionRequest{
    Model: openai.GPT4o,
    Messages: []openai.ChatCompletionMessage{{
      Role:    openai.ChatMessageRoleUser,
      Content: fmt.Sprintf(\`Extract the user. Return JSON with
name (string), email (string), age (int), tier (free|pro|enterprise).

Text: %s\`, text),
    }},
  })
  if err != nil { return nil, err }
  var u User
  if err := json.Unmarshal([]byte(resp.Choices[0].Message.Content), &u); err != nil {
    return nil, err
  }
  return &u, nil
}
`;

const RUST_NATIVE = `use serde::Deserialize;

#[derive(Deserialize)]
struct User {
    name: String,
    email: String,
    age: i64,
    tier: String,
}

async fn extract_user(text: &str) -> anyhow::Result<User> {
    let body = serde_json::json!({
        "model": "gpt-4o",
        "messages": [{
            "role": "user",
            "content": format!(
                "Extract the user. Return JSON with \
                 name (string), email (string), age (int), \
                 tier (free|pro|enterprise).\n\nText: {text}"
            ),
        }],
    });
    let resp: serde_json::Value = reqwest::Client::new()
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(std::env::var("OPENAI_API_KEY")?)
        .json(&body).send().await?.json().await?;
    let raw = resp["choices"][0]["message"]["content"].as_str().unwrap_or("");
    Ok(serde_json::from_str(raw)?)
}
`;

const BAML_USER = `class User {
  name  string
  email string
  age   int
  tier  "free" | "pro" | "enterprise"
}

function ExtractUser(text: string) -> User {
  client GPT4o
  prompt #"
    Extract the user from the text.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ text }}
  "#
}
`;

type Sample = { code: string; lang: 'python' | 'typescript' | 'go' | 'rust' | 'baml'; filename: string };

type Comparison = {
  id: string;
  tab: string;
  headline: string;
  body: string;
  bullets: string[];
  native: Sample;
};

const COMPARISONS: Comparison[] = [
  {
    id: 'python',
    tab: 'Python',
    headline: 'BAML vs Python.',
    body: 'Python is what most agent codebases start with. The OpenAI client returns a string. You parse it. You hope. BAML moves the schema, prompt, and parsing into one typed function.',
    bullets: [
      'No more f-string prompts. The schema lives in the language.',
      'No more JSON.loads then guess. The return type drives parsing.',
      'Same Python app calls BAML through a generated typed client.',
      'Pydantic still works. BAML replaces the prompt and parse boundary, not your data layer.',
    ],
    native: { code: PY_NATIVE, lang: 'python', filename: 'extract.py' },
  },
  {
    id: 'typescript',
    tab: 'TypeScript',
    headline: 'BAML vs TypeScript.',
    body: 'TypeScript types vanish at runtime. Your OpenAI response is any. BAML keeps the type all the way through parsing, so the value you hand to the rest of your app is the type you declared.',
    bullets: [
      'BAML compiles, then generates a typed TS client.',
      'Structured outputs are repaired during decoding, not after.',
      'Streams typed partials into your UI without bespoke JSON parsers.',
      'Drop in next to Zod. Use BAML at the LLM boundary, Zod elsewhere.',
    ],
    native: { code: TS_NATIVE, lang: 'typescript', filename: 'extract.ts' },
  },
  {
    id: 'go',
    tab: 'Go',
    headline: 'BAML vs Go.',
    body: 'Go gives you struct tags and json.Unmarshal. That works for clean JSON. LLMs do not return clean JSON. BAML repairs malformed output during decoding and gives you a typed value.',
    bullets: [
      'BAML generates a typed Go client. Call it like any other function.',
      'Schema and prompt live next to each other in one .baml file.',
      'No more boilerplate ChatCompletionRequest setup per function.',
      'Tests live next to the code. Run with baml test.',
    ],
    native: { code: GO_NATIVE, lang: 'go', filename: 'extract.go' },
  },
  {
    id: 'rust',
    tab: 'Rust',
    headline: 'BAML vs Rust.',
    body: 'Rust with serde and reqwest is fast and safe. It is also a lot of code per LLM call. BAML keeps the safety, drops the boilerplate, and generates a typed client to call from Rust.',
    bullets: [
      'One .baml file replaces request building, JSON parsing, and error plumbing.',
      'Schema-aware parsing handles partial and malformed model output.',
      'Run pure BAML on the BAML VM, or call it from Rust through a generated client.',
      'BAML\'s class layout is contiguous and resolved at compile time.',
    ],
    native: { code: RUST_NATIVE, lang: 'rust', filename: 'extract.rs' },
  },
];

// ── Shiki tokenizer hook ─────────────────────────────────────────────────────

type HighlightInput = {
  code: string;
  lang: 'python' | 'typescript' | 'go' | 'rust' | 'baml';
};
type CodeToken = { content: string; color?: string };
type CodeTokens = CodeToken[][];

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
          langs: [
            'python',
            'typescript',
            'go',
            'rust',
            bamlJinjaTextmate,
            bamlTextmate,
          ],
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
        height: '100%',
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
          <span style={dot('#E5A8A8')} />
          <span style={dot('#E5C58A')} />
          <span style={dot('#A8D0A0')} />
        </div>
        <span
          style={{
            color: MUTED,
            fontFamily: MONO,
            fontSize: 11,
            letterSpacing: '0.04em',
            textAlign: 'center',
          }}
        >
          {filename}
        </span>
        <span />
      </div>

      <div
        style={{
          background: CODE_BG,
          display: 'flex',
          fontFamily: MONO,
          fontSize: 13,
          lineHeight: `${LINE_HEIGHT}px`,
        }}
      >
        <div
          aria-hidden
          style={{
            background: GUTTER_BG,
            borderRight: `1px solid ${BORDER}`,
            color: '#B8B0A0',
            flexShrink: 0,
            fontVariantNumeric: 'tabular-nums',
            padding: `${CODE_PAD_TOP}px 8px`,
            textAlign: 'right',
            userSelect: 'none',
            width: LINE_NUM_WIDTH,
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
            background: 'transparent',
            color: INK,
            flex: 1,
            margin: 0,
            minWidth: 0,
            overflowWrap: 'anywhere',
            padding: `${CODE_PAD_TOP}px ${CODE_PAD_LEFT}px`,
            tabSize: 4,
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}
        >
          <code style={{ background: 'transparent', fontFamily: MONO }}>
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

const dot = (color: string) => ({
  background: color,
  border: '1px solid rgba(0,0,0,0.06)',
  borderRadius: '50%',
  height: 10,
  width: 10,
});

// ── Tab toggle ───────────────────────────────────────────────────────────────

function VsTabs({
  activeId,
  onChange,
}: {
  activeId: string;
  onChange: (id: string) => void;
}) {
  return (
    <div
      role="tablist"
      aria-label="Comparison language"
      style={{
        background: '#F5F1E5',
        border: `1px solid ${BORDER}`,
        borderRadius: 999,
        display: 'inline-flex',
        gap: 4,
        marginTop: 28,
        padding: 4,
      }}
    >
      {COMPARISONS.map((c) => {
        const isActive = c.id === activeId;
        return (
          <button
            aria-selected={isActive}
            key={c.id}
            onClick={() => onChange(c.id)}
            role="tab"
            style={{
              background: isActive ? BG : 'transparent',
              border: 'none',
              borderRadius: 999,
              boxShadow: isActive
                ? '0 1px 2px rgba(0,0,0,0.06), 0 1px 0 rgba(255,255,255,0.6) inset'
                : 'none',
              color: isActive ? INK : MUTED,
              cursor: 'pointer',
              fontFamily: HAND,
              fontSize: 14,
              fontWeight: 500,
              padding: '10px 18px',
              transition:
                'background 200ms ease, color 200ms ease, box-shadow 200ms ease',
            }}
            type="button"
          >
            BAML vs {c.tab}
          </button>
        );
      })}
    </div>
  );
}

// ── Performance section ──────────────────────────────────────────────────────

type Bench = { name: string; speedup: string };
const BENCHES: Bench[] = [
  { name: 'nested field access (500k)', speedup: '7.9x' },
  { name: 'fib iterative (100k)', speedup: '1.2x' },
  { name: 'if-else dispatch (1M)', speedup: '1.2x' },
  { name: 'guard clauses (1M)', speedup: '1.2x' },
  { name: 'collatz (100k)', speedup: '1.2x' },
  { name: 'bubble sort (5k)', speedup: '1.2x' },
  { name: 'class instances (100k)', speedup: '1.1x' },
  { name: 'closure apply (1M)', speedup: '1.1x' },
  { name: 'binary tree depth 20', speedup: '1.1x' },
  { name: 'grid alloc (1000x100)', speedup: '1.1x' },
  { name: 'array build+sum (100k)', speedup: '1.0x' },
  { name: 'string split (100k)', speedup: '1.0x' },
  { name: 'fib(32) recursive', speedup: '1.0x' },
  { name: 'nested loops (500x500)', speedup: '1.0x' },
];

function PerfSection() {
  return (
    <section
      aria-label="BAML performance vs CPython baseline"
      style={{
        background: BG,
        borderTop: `1px solid ${BORDER}`,
        padding: '120px 0 160px',
        width: '100%',
      }}
    >
      <div
        style={{
          margin: '0 auto',
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
          Performance
        </p>
        <h2
          style={{
            fontSize: 'clamp(2rem, 4vw, 3rem)',
            fontWeight: 600,
            letterSpacing: '-0.03em',
            lineHeight: 1.05,
            margin: '12px 0 0',
            maxWidth: 880,
          }}
        >
          7.9x faster than CPython on nested field access.
        </h2>
        <p
          style={{
            color: MUTED,
            fontSize: 18,
            lineHeight: 1.55,
            margin: '20px 0 0',
            maxWidth: 720,
          }}
        >
          BAML class layouts are contiguous and resolved at compile time, so
          field reads compile to direct offsets instead of dictionary lookups.
          On a 500k iteration nested field access loop, BAML runs 7.9x faster
          than CPython. On most other workloads BAML matches CPython within 10
          to 20 percent. These numbers are from the current build and
          improving.
        </p>

        <div
          style={{
            display: 'grid',
            gap: 32,
            gridTemplateColumns: '1fr 1fr',
            marginTop: 56,
          }}
        >
          <div>
            <div
              style={{
                background: '#F5F1E5',
                border: `1px solid ${BORDER}`,
                borderRadius: 12,
                overflow: 'hidden',
              }}
            >
              {BENCHES.map((b, i) => {
                const isHero = b.speedup === '7.9x';
                return (
                  <div
                    key={b.name}
                    style={{
                      alignItems: 'center',
                      background: isHero ? 'rgba(124, 58, 237, 0.08)' : BG,
                      borderTop:
                        i === 0 ? 'none' : `1px solid ${BORDER}`,
                      display: 'grid',
                      gridTemplateColumns: '1fr auto',
                      fontFamily: MONO,
                      fontSize: 13,
                      padding: '12px 18px',
                    }}
                  >
                    <span style={{ color: isHero ? INK : MUTED }}>
                      {b.name}
                    </span>
                    <span
                      style={{
                        color: isHero ? ACCENT : INK,
                        fontVariantNumeric: 'tabular-nums',
                        fontWeight: isHero ? 700 : 500,
                      }}
                    >
                      {b.speedup}
                    </span>
                  </div>
                );
              })}
            </div>
            <p
              style={{
                color: '#8A8580',
                fontSize: 12,
                fontStyle: 'italic',
                margin: '12px 0 0',
              }}
            >
              Speedup vs CPython on the same workload. Higher is faster. Numbers
              from the current BAML VM build.
            </p>
          </div>

          <div style={{ paddingTop: 8 }}>
            <h3
              style={{
                color: INK,
                fontSize: 'clamp(1.25rem, 2vw, 1.5rem)',
                fontWeight: 600,
                letterSpacing: '-0.02em',
                lineHeight: 1.2,
                margin: 0,
              }}
            >
              Why nested field access wins.
            </h3>
            <ul
              style={{
                color: MUTED,
                display: 'flex',
                flexDirection: 'column',
                fontSize: 15,
                gap: 14,
                lineHeight: 1.55,
                listStyle: 'none',
                margin: '20px 0 0',
                padding: 0,
              }}
            >
              <li style={bulletStyle}>
                <span style={dotMark} />
                <span>
                  Classes have a fixed shape known at compile time. No hash
                  table per access.
                </span>
              </li>
              <li style={bulletStyle}>
                <span style={dotMark} />
                <span>
                  The TIR layer resolves <code style={inlineCode}>a.b.c</code>{' '}
                  to a fixed offset chain before the bytecode runs.
                </span>
              </li>
              <li style={bulletStyle}>
                <span style={dotMark} />
                <span>
                  Agent code reads object fields constantly. Tools, history,
                  message lists, tagged variants. The hot path benefits.
                </span>
              </li>
              <li style={bulletStyle}>
                <span style={dotMark} />
                <span>
                  Most other workloads sit at 1.0x to 1.2x today. The compiler
                  is young. The VM is improving.
                </span>
              </li>
            </ul>
          </div>
        </div>
      </div>
    </section>
  );
}

const bulletStyle = {
  alignItems: 'baseline' as const,
  display: 'flex',
  gap: 12,
};
const dotMark = {
  background: ACCENT,
  borderRadius: 999,
  flexShrink: 0,
  height: 6,
  marginTop: 8,
  width: 6,
};
const inlineCode = {
  background: '#EDE6D6',
  borderRadius: 3,
  fontFamily: MONO,
  fontSize: '0.9em',
  padding: '1px 5px',
};

// ── Comparison block ─────────────────────────────────────────────────────────

function ComparisonView({
  comparison,
  nativeTokens,
  bamlTokens,
}: {
  comparison: Comparison;
  nativeTokens: CodeTokens;
  bamlTokens: CodeTokens;
}) {
  const reduced = useReducedMotion();
  const fadeT = reduced ? { duration: 0 } : { duration: 0.35 };

  return (
    <AnimatePresence initial={false} mode="wait">
      <motion.div
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -4 }}
        initial={{ opacity: 0, y: 4 }}
        key={`cmp-${comparison.id}`}
        transition={fadeT}
      >
        <h3
          style={{
            color: INK,
            fontSize: 'clamp(1.5rem, 2.4vw, 2rem)',
            fontWeight: 600,
            letterSpacing: '-0.02em',
            lineHeight: 1.15,
            margin: 0,
          }}
        >
          {comparison.headline}
        </h3>
        <p
          style={{
            color: MUTED,
            fontSize: 17,
            lineHeight: 1.6,
            margin: '14px 0 0',
            maxWidth: 720,
          }}
        >
          {comparison.body}
        </p>

        <div
          style={{
            display: 'grid',
            gap: 20,
            gridTemplateColumns: '1fr 1fr',
            marginTop: 32,
          }}
        >
          <div>
            <div style={miniLabelLeft}>{comparison.tab}</div>
            <CodeBlock
              filename={comparison.native.filename}
              tokens={nativeTokens}
            />
          </div>
          <div>
            <div style={miniLabelRight}>BAML</div>
            <CodeBlock filename="extract.baml" tokens={bamlTokens} />
          </div>
        </div>

        <ul
          style={{
            color: MUTED,
            display: 'grid',
            fontSize: 14.5,
            gap: 12,
            gridTemplateColumns: '1fr 1fr',
            lineHeight: 1.55,
            listStyle: 'none',
            margin: '32px 0 0',
            padding: 0,
          }}
        >
          {comparison.bullets.map((b) => (
            <li key={b} style={bulletStyle}>
              <span style={dotMark} />
              <span>{b}</span>
            </li>
          ))}
        </ul>
      </motion.div>
    </AnimatePresence>
  );
}

const miniLabelLeft = {
  color: '#8A8580',
  fontFamily: MONO,
  fontSize: 11,
  fontWeight: 500,
  letterSpacing: '0.12em',
  marginBottom: 8,
  textTransform: 'uppercase' as const,
};
const miniLabelRight = {
  ...miniLabelLeft,
  color: ACCENT,
};

// ── Main ─────────────────────────────────────────────────────────────────────

export function VsClient() {
  const [activeId, setActiveId] = useState<string>(COMPARISONS[0].id);

  // Tokenize all native samples + the shared BAML sample once.
  const tokens = useTokenized([
    { code: PY_NATIVE, lang: 'python' },
    { code: TS_NATIVE, lang: 'typescript' },
    { code: GO_NATIVE, lang: 'go' },
    { code: RUST_NATIVE, lang: 'rust' },
    { code: BAML_USER, lang: 'baml' },
  ]);

  const nativeByLang: Record<string, CodeTokens> = {
    python: tokens[0],
    typescript: tokens[1],
    go: tokens[2],
    rust: tokens[3],
  };
  const bamlTokens = tokens[4];

  const active =
    COMPARISONS.find((c) => c.id === activeId) ?? COMPARISONS[0];

  return (
    <>
      <section
        aria-label="BAML vs other languages"
        style={{
          background: BG,
          color: INK,
          padding: '120px 0 80px',
          width: '100%',
        }}
      >
        <div
          style={{
            margin: '0 auto',
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
            BAML vs X
          </p>
          <h1
            style={{
              fontSize: 'clamp(2.25rem, 4.5vw, 3.5rem)',
              fontWeight: 600,
              letterSpacing: '-0.03em',
              lineHeight: 1.05,
              margin: '12px 0 0',
              maxWidth: 920,
            }}
          >
            How BAML compares.
          </h1>
          <p
            style={{
              color: MUTED,
              fontSize: 18,
              lineHeight: 1.55,
              margin: '20px 0 0',
              maxWidth: 720,
            }}
          >
            Same task, four languages. Extract a typed user from a string.
            Notice where the schema lives, where parsing happens, and how much
            code is doing structural work versus business logic.
          </p>

          <VsTabs activeId={activeId} onChange={setActiveId} />
        </div>

        <div
          style={{
            margin: '64px auto 0',
            maxWidth: 1360,
            padding: '0 32px 0 48px',
          }}
        >
          <ComparisonView
            bamlTokens={bamlTokens}
            comparison={active}
            nativeTokens={nativeByLang[active.id] ?? tokens[0]}
          />
        </div>
      </section>

      <PerfSection />
      <ClosingCta />
    </>
  );
}

// ── Closing CTA ──────────────────────────────────────────────────────────────

const INSTALL_PROMPT = `Set up BAML in this project.

1. Install the BAML CLI for my OS.
2. Run \`baml init\` and add baml_src/ to the project.
3. Create one example typed function: a class for the schema,
   a function block with a prompt and a client, and a generator
   block targeting the language this project already uses.
4. Run \`baml generate\` and show me how to call the function
   from my code.

Use https://docs.boundaryml.com as the source of truth. Ask me
which LLM provider to wire up before writing the client block.`;

function ClosingCta() {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(INSTALL_PROMPT);
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      /* clipboard blocked */
    }
  };

  return (
    <section
      style={{
        background: '#FAF7EF',
        borderTop: `1px solid ${BORDER}`,
        padding: '120px 0 140px',
        width: '100%',
      }}
    >
      <div
        style={{
          margin: '0 auto',
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
          Try it
        </p>
        <h2
          style={{
            fontSize: 'clamp(2rem, 4vw, 3rem)',
            fontWeight: 600,
            letterSpacing: '-0.03em',
            lineHeight: 1.05,
            margin: '12px 0 0',
            maxWidth: 720,
          }}
        >
          Skip the CLI. Tell your agent to install it.
        </h2>

        <div
          style={{
            display: 'grid',
            gap: 32,
            gridTemplateColumns: '1fr 1fr',
            marginTop: 40,
          }}
        >
          <div
            style={{
              background: CODE_BG,
              border: `1px solid ${BORDER}`,
              borderRadius: 10,
              boxShadow:
                '0 1px 0 rgba(0,0,0,0.02), 0 8px 28px -18px rgba(0,0,0,0.22)',
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                alignItems: 'center',
                background: GUTTER_BG,
                borderBottom: `1px solid ${BORDER}`,
                color: MUTED,
                display: 'flex',
                fontFamily: MONO,
                fontSize: 11,
                gap: 12,
                justifyContent: 'space-between',
                letterSpacing: '0.06em',
                padding: '8px 10px 8px 14px',
                textTransform: 'uppercase' as const,
              }}
            >
              <span>paste into Claude Code or Cursor</span>
              <button
                onClick={handleCopy}
                style={{
                  background: copied ? ACCENT : '#ffffff',
                  border: `1px solid ${copied ? ACCENT : BORDER}`,
                  borderRadius: 6,
                  color: copied ? '#ffffff' : MUTED,
                  cursor: 'pointer',
                  fontFamily: MONO,
                  fontSize: 10.5,
                  fontWeight: 500,
                  letterSpacing: '0.08em',
                  padding: '4px 10px',
                  textTransform: 'uppercase' as const,
                  transition:
                    'background 160ms ease, color 160ms ease, border-color 160ms ease',
                }}
                type="button"
              >
                {copied ? 'Copied' : 'Copy prompt'}
              </button>
            </div>
            <pre
              style={{
                color: INK,
                fontFamily: MONO,
                fontSize: 12.5,
                lineHeight: 1.6,
                margin: 0,
                padding: '14px 18px',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
              }}
            >
              <code style={{ fontFamily: MONO }}>{INSTALL_PROMPT}</code>
            </pre>
          </div>

          <div
            style={{
              alignItems: 'flex-start',
              display: 'flex',
              flexDirection: 'column',
              gap: 18,
              paddingTop: 4,
            }}
          >
            <p
              style={{
                color: MUTED,
                fontSize: 17,
                lineHeight: 1.6,
                margin: 0,
                maxWidth: 460,
              }}
            >
              Drop this into Claude Code, Cursor, or any coding agent. It will
              install the CLI, scaffold a typed function, generate the client
              for your language, and show you how to call it. No man pages, no
              flag spelunking.
            </p>
            <div
              style={{
                display: 'flex',
                flexWrap: 'wrap',
                gap: 12,
                marginTop: 6,
              }}
            >
              <a
                href="https://docs.boundaryml.com"
                style={{
                  alignItems: 'center',
                  background: ACCENT,
                  borderRadius: 999,
                  color: '#ffffff',
                  display: 'inline-flex',
                  fontFamily: HAND,
                  fontSize: 14,
                  fontWeight: 500,
                  gap: 8,
                  padding: '12px 22px',
                  textDecoration: 'none',
                }}
              >
                Read the docs
                <span aria-hidden style={{ fontSize: 16, lineHeight: 1 }}>
                  →
                </span>
              </a>
              <a
                href="https://github.com/BoundaryML/baml"
                style={{
                  alignItems: 'center',
                  background: '#ffffff',
                  border: `1px solid ${BORDER}`,
                  borderRadius: 999,
                  color: INK,
                  display: 'inline-flex',
                  fontFamily: HAND,
                  fontSize: 14,
                  fontWeight: 500,
                  gap: 8,
                  padding: '12px 22px',
                  textDecoration: 'none',
                }}
              >
                Star on GitHub
              </a>
              <a
                href="/thesis"
                style={{
                  alignItems: 'center',
                  background: 'transparent',
                  borderRadius: 999,
                  color: ACCENT,
                  display: 'inline-flex',
                  fontFamily: HAND,
                  fontSize: 14,
                  fontWeight: 500,
                  gap: 8,
                  padding: '12px 4px',
                  textDecoration: 'none',
                }}
              >
                Read the thesis
                <span aria-hidden style={{ fontSize: 16, lineHeight: 1 }}>
                  →
                </span>
              </a>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
