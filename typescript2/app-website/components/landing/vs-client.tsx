'use client';

import { AnimatePresence, motion, useReducedMotion } from 'motion/react';
import Image from 'next/image';
import { useEffect, useState } from 'react';
import { ScriptCopyBtn } from '../magicui/script-copy-btn';

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

def qualify_lead(email: str):
    response = client.chat.completions.create(
        model="gpt-4o",
        messages=[{
            "role": "user",
            "content": f"""Qualify this sales lead. Return JSON:
contact: {{ name, email, role }}
company: {{ name, size: startup|midmarket|enterprise }}
signals: array of {{ label, score 0-100, evidence }}
next_action: ignore|nurture|book_demo|escalate
follow_up: short email draft

Email: {email}"""
        }]
    )
    try:
        data = json.loads(response.choices[0].message.content)
        if data["next_action"] == "escalate" and len(data["signals"]) == 0:
            raise ValueError("missing evidence")
        return data
    except json.JSONDecodeError:
        return None
`;

const TS_NATIVE = `import OpenAI from "openai";

const client = new OpenAI();

async function qualifyLead(email: string) {
  const r = await client.chat.completions.create({
    model: "gpt-4o",
    messages: [{
      role: "user",
      content: \`Qualify this sales lead. Return JSON:
contact: { name, email, role }
company: { name, size: startup|midmarket|enterprise }
signals: [{ label, score, evidence }]
next_action: ignore|nurture|book_demo|escalate
follow_up: short email draft

Email: \${email}\`,
    }],
  });
  try {
    const lead = JSON.parse(r.choices[0].message.content ?? "");
    if (lead.next_action === "escalate" && !lead.signals?.length) {
      throw new Error("missing evidence");
    }
    return lead;
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

type Signal struct {
  Label    string \`json:"label"\`
  Score    int    \`json:"score"\`
  Evidence string \`json:"evidence"\`
}

type Lead struct {
  Contact    map[string]string \`json:"contact"\`
  Company    map[string]string \`json:"company"\`
  Signals    []Signal          \`json:"signals"\`
  NextAction string            \`json:"next_action"\`
  FollowUp   string            \`json:"follow_up"\`
}

func QualifyLead(email string) (*Lead, error) {
  c := openai.NewClient("OPENAI_API_KEY")
  resp, err := c.CreateChatCompletion(context.Background(), openai.ChatCompletionRequest{
    Model: openai.GPT4o,
    Messages: []openai.ChatCompletionMessage{{
      Role:    openai.ChatMessageRoleUser,
      Content: fmt.Sprintf(\`Qualify this lead as JSON with
contact, company, signals[], next_action, follow_up.
next_action is ignore|nurture|book_demo|escalate.

Email: %s\`, email),
    }},
  })
  if err != nil { return nil, err }
  var lead Lead
  if err := json.Unmarshal([]byte(resp.Choices[0].Message.Content), &lead); err != nil {
    return nil, err
  }
  return &lead, nil
}
`;

const RUST_NATIVE = `use serde::Deserialize;

#[derive(Deserialize)]
struct Signal {
    label: String,
    score: i64,
    evidence: String,
}

#[derive(Deserialize)]
struct Contact {
    name: String,
    email: String,
    role: String,
}

#[derive(Deserialize)]
struct Company {
    name: String,
    size: String,
}

#[derive(Deserialize)]
struct Lead {
    contact: Contact,
    company: Company,
    signals: Vec<Signal>,
    next_action: String,
    follow_up: String,
}

async fn qualify_lead(email: &str) -> anyhow::Result<Lead> {
    let body = serde_json::json!({
        "model": "gpt-4o",
        "messages": [{
            "role": "user",
            "content": format!(
                "Qualify this lead. Return JSON with contact, \
                 company, signals[], next_action, follow_up. \
                 next_action is ignore|nurture|book_demo|escalate.\n\n\
                 Email: {email}"
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

const BAML_USER = `class LeadDecision {
  contact_email string
  company       string
  signals       string[] @description("Evidence from the email")
  score         int @description("0 to 100")
  next_action   "ignore" | "nurture" | "book_demo" | "escalate"
  rationale     string
  follow_up     string
}

function QualifyLead(email: string) -> LeadDecision {
  client GPT4o
  prompt #"
    Decide if sales should act on this inbound lead.
    Use evidence, explain the score, and draft the next message.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ email }}
  "#
}
`;

const BAML_WORKFLOW = `class TicketDecision {
  intent   "refund" | "bug" | "upgrade" | "question"
  priority "low" | "medium" | "high"
  reason   string @description("Why this route is correct")
  reply    string
}

function HandleTicket(text: string) -> TicketDecision {
  client GPT4o
  prompt #"
    Classify, decide priority, explain the route, and draft a reply.
    Escalate high-priority refunds and bugs.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ text }}
  "#
}
`;

const PY_CALLER = `from baml_client import b

lead = b.QualifyLead(inbound_email)
if lead.next_action == "book_demo":
    crm.create_task(lead.contact_email, lead.follow_up)
`;

const TS_CALLER = `import { b } from "@/baml_client";

const lead = await b.QualifyLead(inboundEmail);
if (lead.next_action === "book_demo") {
  await crm.createTask(lead.contact_email, lead.follow_up);
}
`;

const GO_CALLER = `package main

import "context"
import b "example.com/app/baml_client"

func main() {
  lead, err := b.QualifyLead(context.Background(), inboundEmail)
  if err != nil { panic(err) }
  println(lead.NextAction)
}
`;

const RUST_CALLER = `use baml_client::b;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let lead = b::qualify_lead(inbound_email()).await?;
    println!("{}", lead.next_action);
    Ok(())
}
`;

const PY_WORKFLOW_CALLER = `from baml_client import b

decision = b.HandleTicket("I was charged twice and need a refund today")
print(decision.priority, decision.reason)
`;

const TS_WORKFLOW_CALLER = `import { b } from "@/baml_client";

export async function POST(req: Request) {
  const { ticket } = await req.json();
  const decision = await b.HandleTicket(ticket);
  return Response.json(decision);
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
    body: 'Python is what most agent codebases start with. Once the output is nested, scored, and action-oriented, the OpenAI client still returns a string. BAML moves the schema, prompt, evidence requirements, and parsing into one typed function.',
    bullets: [
      'The BAML version asks for the app decision, evidence, rationale, and next action directly.',
      'No more JSON.loads plus manual evidence checks. The return type drives parsing.',
      'Same Python app calls BAML through a generated typed client.',
      'Pydantic still works. BAML replaces the prompt and parse boundary, not your data layer.',
    ],
    baml: { code: BAML_USER, lang: 'baml', filename: 'lead.baml' },
    caller: { code: PY_CALLER, lang: 'python', filename: 'app.py' },
    native: { code: PY_NATIVE, lang: 'python', filename: 'lead.py' },
  },
  {
    id: 'typescript',
    tab: 'TypeScript',
    headline: 'BAML vs TypeScript.',
    body: 'TypeScript types vanish at runtime. When the model returns a recommended action plus scored evidence, your OpenAI response is still any. BAML keeps the declared type through parsing and into the generated client.',
    bullets: [
      'BAML compiles, then generates a typed TS client.',
      'The output shape includes rationale, so app code branches on an explained decision.',
      'Streams typed partials into your UI without bespoke JSON parsers.',
      'Drop in next to Zod. Use BAML at the LLM boundary, Zod elsewhere.',
    ],
    baml: { code: BAML_USER, lang: 'baml', filename: 'lead.baml' },
    caller: { code: TS_CALLER, lang: 'typescript', filename: 'route.ts' },
    native: { code: TS_NATIVE, lang: 'typescript', filename: 'lead.ts' },
  },
  {
    id: 'go',
    tab: 'Go',
    headline: 'BAML vs Go.',
    body: 'Go gives you struct tags and json.Unmarshal. That works for clean JSON. LLMs often return explanations, missing fields, or fuzzy enum values. BAML keeps the prompt, schema, and decoder together and gives Go a typed value.',
    bullets: [
      'BAML generates a typed Go client. Call it like any other function.',
      'Reasoning requirements live with the prompt and type, not in comments around json.Unmarshal.',
      'No more boilerplate ChatCompletionRequest setup per function.',
      'Tests live next to the code. Run with baml test.',
    ],
    baml: { code: BAML_USER, lang: 'baml', filename: 'lead.baml' },
    caller: { code: GO_CALLER, lang: 'go', filename: 'main.go' },
    native: { code: GO_NATIVE, lang: 'go', filename: 'lead.go' },
  },
  {
    id: 'rust',
    tab: 'Rust',
    headline: 'BAML vs Rust.',
    body: 'Rust with serde and reqwest is fast and safe. The friction is all the LLM-specific glue: request construction, prompt/schema drift, repair, and error paths. BAML keeps the safety and generates the typed Rust-facing boundary.',
    bullets: [
      'One .baml file replaces request building, JSON parsing, and error plumbing.',
      'Schema-aware parsing handles partial and malformed model output before Rust receives it.',
      'Run pure BAML on the BAML VM, or call it from Rust through a generated client.',
      'The generated Rust-facing type includes the decision, evidence, and rationale.',
    ],
    baml: { code: BAML_USER, lang: 'baml', filename: 'lead.baml' },
    caller: { code: RUST_CALLER, lang: 'rust', filename: 'main.rs' },
    native: { code: RUST_NATIVE, lang: 'rust', filename: 'lead.rs' },
  },
  {
    id: 'langgraph',
    tab: 'LangGraph',
    headline: 'BAML vs LangGraph.',
    body: 'LangGraph is a graph runtime. BAML is the typed language boundary around the model calls inside that runtime. For many workflows, the BAML version is just functions, control flow, tests, and generated clients.',
    bullets: [
      'Use BAML inside LangGraph when you need a graph, or use BAML alone when the workflow is mostly typed model calls.',
      'The prompt, output type, explanation field, and tests live in the same file.',
      'For simple workflows, one BAML function can replace classify, route, draft, and parse nodes.',
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
      'BAML functions return reusable decisions, not route-local generated objects.',
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

type InstallPath = 'claude' | 'codex';

const installOptions: {
  command: string;
  icon?: string;
  id: InstallPath;
  label: string;
}[] = [
  {
    command: 'claude plugin add boundaryml/baml',
    icon: '/Claude Color SVG.svg',
    id: 'claude',
    label: 'Claude plugin',
  },
  {
    command: 'codex plugin add boundaryml/baml',
    icon: '/Codex Color.svg',
    id: 'codex',
    label: 'Codex plugin',
  },
];

function ClosingCta() {
  const [installPath, setInstallPath] = useState<InstallPath>('claude');
  const selected =
    installOptions.find((option) => option.id === installPath) ??
    installOptions[0];

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

        <div className="cta-grid">
          <div style={{ alignSelf: 'start' }}>
            <ScriptCopyBtn
              className="block w-full max-w-md"
              codeLanguage="bash"
              commandMap={{ bash: selected.command } as const}
              darkTheme="none"
              lightTheme="none"
              showMultiplePackageOptions={false}
            />
            <div className="plugin-pill-row">
              {installOptions.map((option) => {
                const isActive = installPath === option.id;
                return (
                  <button
                    className={`plugin-pill${isActive ? ' plugin-pill--active' : ''}`}
                    key={option.id}
                    onClick={() => {
                      setInstallPath(option.id);
                    }}
                    type="button"
                  >
                    {option.icon && (
                      <Image
                        alt={option.label}
                        className="size-4"
                        height={16}
                        src={option.icon}
                        width={16}
                      />
                    )}
                    {option.label}
                  </button>
                );
              })}
            </div>
            <div className="manual-install">
              <span className="manual-install__label">Prefer manual setup?</span>
              <a
                className="manual-install__link"
                href="https://docs.boundaryml.com/guide/installation-language/rest-api-other-languages"
                rel="noreferrer"
                target="_blank"
              >
                See manual install instructions
                <span aria-hidden>→</span>
              </a>
            </div>
          </div>

          <div className="cta-right">
            <p
              style={{
                color: MUTED,
                fontSize: 17,
                lineHeight: 1.6,
                margin: 0,
                maxWidth: 460,
              }}
            >
              Pick the agent you use, install the BAML plugin, then ask it to
              add one typed LLM function to your codebase. The plugin gives the
              agent BAML-specific context instead of making it infer the syntax
              from scratch.
            </p>
            <div className="cta-row">
              <a
                className="editorial-btn editorial-btn--primary"
                href="https://docs.boundaryml.com"
                rel="noreferrer"
                target="_blank"
              >
                Read the docs
                <span aria-hidden>→</span>
              </a>
              <a
                className="editorial-btn"
                href="https://github.com/BoundaryML/baml"
                rel="noreferrer"
                target="_blank"
              >
                Star on GitHub
              </a>
              <a
                className="editorial-btn editorial-btn--ghost"
                href="/thesis"
              >
                Read the thesis
                <span aria-hidden>→</span>
              </a>
            </div>
          </div>
        </div>
      </div>

      <style>{`
        .cta-grid {
          display: grid;
          gap: 32px;
          grid-template-columns: 1fr 1fr;
          margin-top: 40px;
        }
        .cta-right {
          align-items: flex-start;
          display: flex;
          flex-direction: column;
          gap: 18px;
          padding-top: 4px;
        }
        .cta-row {
          display: flex;
          flex-wrap: wrap;
          gap: 12px;
          margin-top: 6px;
        }
        .plugin-pill-row {
          display: flex;
          flex-wrap: wrap;
          gap: 8px;
          margin-top: 12px;
        }
        .plugin-pill {
          align-items: center;
          background: transparent;
          border: 1px solid ${BORDER};
          border-radius: 8px;
          color: ${MUTED};
          cursor: pointer;
          display: inline-flex;
          font-family: ${HAND};
          font-size: 14px;
          font-weight: 500;
          gap: 6px;
          padding: 10px 16px;
          transition: background-color 200ms ease, border-color 200ms ease, color 200ms ease;
        }
        .plugin-pill:hover {
          border-color: ${ACCENT};
          color: ${ACCENT};
        }
        .plugin-pill--active,
        .plugin-pill--active:hover {
          background: ${INK};
          border-color: ${INK};
          color: #ffffff;
        }
        .editorial-btn {
          align-items: center;
          background: #ffffff;
          border: 1px solid ${BORDER};
          border-radius: 999px;
          color: ${INK};
          display: inline-flex;
          font-family: ${HAND};
          font-size: 14px;
          font-weight: 500;
          gap: 8px;
          letter-spacing: 0.01em;
          padding: 12px 22px;
          text-decoration: none;
          transition: background-color 200ms ease, border-color 200ms ease, color 200ms ease, transform 200ms ease;
        }
        .editorial-btn span[aria-hidden] {
          font-size: 16px;
          line-height: 1;
        }
        .editorial-btn:hover {
          background: #FBF8F1;
          border-color: ${ACCENT};
          color: ${ACCENT};
          transform: translateY(-1px);
        }
        .editorial-btn--primary {
          background: ${ACCENT};
          border-color: ${ACCENT};
          color: #ffffff;
        }
        .editorial-btn--primary:hover {
          background: #5B21B6;
          border-color: #5B21B6;
          color: #ffffff;
        }
        .editorial-btn--ghost {
          background: transparent;
          border-color: transparent;
          color: ${ACCENT};
          padding: 12px 4px;
        }
        .editorial-btn--ghost:hover {
          background: transparent;
          border-color: transparent;
          color: #5B21B6;
        }
        .manual-install {
          align-items: center;
          color: ${MUTED};
          display: flex;
          flex-wrap: wrap;
          font-family: ${HAND};
          font-size: 13px;
          gap: 6px;
          margin-top: 20px;
        }
        .manual-install__label {
          color: #8A8580;
        }
        .manual-install__link {
          align-items: center;
          color: ${ACCENT};
          display: inline-flex;
          gap: 4px;
          text-decoration: none;
          transition: color 200ms ease;
        }
        .manual-install__link:hover {
          color: #5B21B6;
          text-decoration: underline;
          text-underline-offset: 2px;
        }
        @media (max-width: 900px) {
          .cta-grid {
            grid-template-columns: 1fr;
            gap: 40px;
          }
        }
        @media (max-width: 640px) {
          .cta-row {
            flex-direction: column;
            align-items: stretch;
            gap: 10px;
          }
          .editorial-btn {
            justify-content: center;
            width: 100%;
          }
          .editorial-btn--ghost {
            padding: 12px 22px;
          }
          .plugin-pill-row {
            flex-direction: column;
            align-items: stretch;
          }
          .plugin-pill {
            justify-content: center;
            width: 100%;
          }
        }
      `}</style>
    </section>
  );
}
