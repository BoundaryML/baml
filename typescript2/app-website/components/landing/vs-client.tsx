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

const LANGGRAPH_NATIVE = `from typing import Literal, TypedDict
from langgraph.graph import END, StateGraph
from openai import OpenAI
import json

client = OpenAI()

class State(TypedDict):
    ticket: str
    intent: str
    priority: str
    reply: str

def classify(state: State):
    raw = client.chat.completions.create(
        model="gpt-4o",
        messages=[{"role": "user", "content": f"""
Classify this ticket as refund, bug, upgrade, or question.
Also return priority low, medium, or high as JSON.

Ticket: {state["ticket"]}
"""}],
    ).choices[0].message.content
    parsed = json.loads(raw)
    return {"intent": parsed["intent"], "priority": parsed["priority"]}

def route(state: State) -> Literal["draft", "escalate"]:
    return "escalate" if state["priority"] == "high" else "draft"

def draft(state: State):
    return {"reply": f"We can help with your {state['intent']} request."}

graph = StateGraph(State)
graph.add_node("classify", classify)
graph.add_node("draft", draft)
graph.add_node("escalate", lambda s: {"reply": "Escalating to support."})
graph.set_entry_point("classify")
graph.add_conditional_edges("classify", route)
graph.add_edge("draft", END)
graph.add_edge("escalate", END)
app = graph.compile()
`;

const AI_SDK_NATIVE = `import { generateObject, streamText } from "ai";
import { openai } from "@ai-sdk/openai";
import { z } from "zod";

const Ticket = z.object({
  intent: z.enum(["refund", "bug", "upgrade", "question"]),
  priority: z.enum(["low", "medium", "high"]),
});

export async function POST(req: Request) {
  const { ticket } = await req.json();

  const triage = await generateObject({
    model: openai("gpt-4o"),
    schema: Ticket,
    prompt: \`Classify this support ticket: \${ticket}\`,
  });

  const reply = streamText({
    model: openai("gpt-4o"),
    prompt: \`Write a support reply for:
intent: \${triage.object.intent}
priority: \${triage.object.priority}
ticket: \${ticket}\`,
  });

  return reply.toDataStreamResponse();
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

const BAML_WORKFLOW = `class Ticket {
  intent   "refund" | "bug" | "upgrade" | "question"
  priority "low" | "medium" | "high"
  summary  string
}

function ClassifyTicket(text: string) -> Ticket {
  client GPT4o
  prompt #"
    Classify this support ticket.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ text }}
  "#
}

function DraftReply(text: string, ticket: Ticket) -> string {
  client GPT4o
  prompt #"
    Write a concise support reply.
    Intent: {{ ticket.intent }}
    Priority: {{ ticket.priority }}
    Ticket: {{ text }}
  "#
}

function HandleTicket(text: string) -> string {
  let ticket = ClassifyTicket(text)
  if (ticket.priority == "high") {
    return "Escalating: " + ticket.summary
  }
  DraftReply(text, ticket)
}
`;

const PY_CALLER = `from baml_client import b

user = b.ExtractUser("Ada, ada@example.com, pro, 37")
print(user.email)
`;

const TS_CALLER = `import { b } from "@/baml_client";

const user = await b.ExtractUser("Ada, ada@example.com, pro, 37");
console.log(user.email);
`;

const GO_CALLER = `package main

import "context"
import b "example.com/app/baml_client"

func main() {
  user, err := b.ExtractUser(context.Background(), "Ada, ada@example.com, pro, 37")
  if err != nil { panic(err) }
  println(user.Email)
}
`;

const RUST_CALLER = `use baml_client::b;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let user = b::extract_user("Ada, ada@example.com, pro, 37").await?;
    println!("{}", user.email);
    Ok(())
}
`;

const PY_WORKFLOW_CALLER = `from baml_client import b

reply = b.HandleTicket("I was charged twice and need a refund today")
print(reply)
`;

const TS_WORKFLOW_CALLER = `import { b } from "@/baml_client";

export async function POST(req: Request) {
  const { ticket } = await req.json();
  const reply = await b.HandleTicket(ticket);
  return Response.json({ reply });
}
`;

type Sample = {
  code: string;
  filename: string;
  lang: 'python' | 'typescript' | 'go' | 'rust' | 'baml';
};

type Comparison = {
  id: string;
  tab: string;
  headline: string;
  body: string;
  bullets: string[];
  baml: Sample;
  caller: Sample;
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
    baml: { code: BAML_USER, lang: 'baml', filename: 'extract.baml' },
    caller: { code: PY_CALLER, lang: 'python', filename: 'app.py' },
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
    baml: { code: BAML_USER, lang: 'baml', filename: 'extract.baml' },
    caller: { code: TS_CALLER, lang: 'typescript', filename: 'route.ts' },
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
    baml: { code: BAML_USER, lang: 'baml', filename: 'extract.baml' },
    caller: { code: GO_CALLER, lang: 'go', filename: 'main.go' },
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
      "BAML's class layout is contiguous and resolved at compile time.",
    ],
    baml: { code: BAML_USER, lang: 'baml', filename: 'extract.baml' },
    caller: { code: RUST_CALLER, lang: 'rust', filename: 'main.rs' },
    native: { code: RUST_NATIVE, lang: 'rust', filename: 'extract.rs' },
  },
  {
    id: 'langgraph',
    tab: 'LangGraph',
    headline: 'BAML vs LangGraph.',
    body: 'LangGraph is a graph runtime. BAML is the typed language boundary around the model calls inside that runtime. For many workflows, the BAML version is just functions, control flow, tests, and generated clients.',
    bullets: [
      'Use BAML inside LangGraph when you need a graph, or use BAML alone when the workflow is mostly typed model calls.',
      'The prompt, schema, parsing, and tests live in the same file.',
      'Control flow can be plain BAML functions instead of graph nodes for simple workflows.',
      'Generated Python keeps app code small and typed.',
    ],
    baml: { code: BAML_WORKFLOW, lang: 'baml', filename: 'support.baml' },
    caller: {
      code: PY_WORKFLOW_CALLER,
      lang: 'python',
      filename: 'support_flow.py',
    },
    native: { code: LANGGRAPH_NATIVE, lang: 'python', filename: 'graph.py' },
  },
  {
    id: 'ai-sdk',
    tab: 'AI SDK',
    headline: 'BAML vs AI SDK.',
    body: 'AI SDK is great at request and streaming plumbing. BAML is better at making the model boundary a reusable typed function with tests, providers, and generated clients.',
    bullets: [
      'Keep AI SDK for UI streaming, use BAML for prompts and typed outputs.',
      'BAML functions are reusable outside one route handler.',
      'Provider switching is a client block, not a rewrite across handlers.',
      'Tests sit next to the function instead of in app-layer fixtures.',
    ],
    baml: { code: BAML_WORKFLOW, lang: 'baml', filename: 'support.baml' },
    caller: {
      code: TS_WORKFLOW_CALLER,
      lang: 'typescript',
      filename: 'route.ts',
    },
    native: { code: AI_SDK_NATIVE, lang: 'typescript', filename: 'route.ts' },
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
// ── Comparison block ─────────────────────────────────────────────────────────

function ComparisonView({
  bamlCallTokens,
  comparison,
  nativeTokens,
  bamlTokens,
}: {
  bamlCallTokens: CodeTokens;
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
            <div style={{ display: 'grid', gap: 14 }}>
              <CodeBlock
                filename={comparison.baml.filename}
                tokens={bamlTokens}
              />
              <CodeBlock
                filename={comparison.caller.filename}
                tokens={bamlCallTokens}
              />
            </div>
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

  const tokens = useTokenized(
    COMPARISONS.flatMap((comparison) => [
      comparison.native,
      comparison.baml,
      comparison.caller,
    ]),
  );

  const active = COMPARISONS.find((c) => c.id === activeId) ?? COMPARISONS[0];
  const activeIndex = COMPARISONS.findIndex((c) => c.id === active.id);
  const tokenOffset = Math.max(activeIndex, 0) * 3;
  const nativeTokens = tokens[tokenOffset] ?? [];
  const bamlTokens = tokens[tokenOffset + 1] ?? [];
  const bamlCallTokens = tokens[tokenOffset + 2] ?? [];

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
            Same task, multiple stacks. Notice where the schema lives, where
            parsing happens, how workflows are expressed, and what the calling
            code looks like after BAML generates the client.
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
            bamlCallTokens={bamlCallTokens}
            bamlTokens={bamlTokens}
            comparison={active}
            nativeTokens={nativeTokens}
          />
        </div>
      </section>

      <ClosingCta />
    </>
  );
}

// ── Closing CTA ──────────────────────────────────────────────────────────────

const INSTALL_PROMPT =
  'claude plugin add boundaryml/baml && claude "Use the BAML plugin to add one typed LLM function to this codebase."';

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
              <span>same Claude plugin install as the homepage</span>
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
              This matches the homepage install path. The agent gets the BAML
              plugin, adds one typed LLM function, generates the client for your
              language, and shows the app-side call site.
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
