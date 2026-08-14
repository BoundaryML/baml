'use client';

import { Check, Copy, Sparkles, Workflow } from 'lucide-react';
import { AnimatePresence, motion, useReducedMotion } from 'motion/react';
import Image from 'next/image';
import { useEffect, useState } from 'react';
import type { IconType } from 'react-icons';
import { SiGo, SiRust, SiTypescript } from 'react-icons/si';
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

const PY_NATIVE = `from typing import Literal
from openai import OpenAI
from pydantic import BaseModel

client = OpenAI()

class Contact(BaseModel):
    name: str
    email: str
    role: str

class Company(BaseModel):
    name: str
    size: Literal["startup", "midmarket", "enterprise"]

class Signal(BaseModel):
    label: str
    score: int
    evidence: str

class Lead(BaseModel):
    contact: Contact
    company: Company
    signals: list[Signal]
    next_action: Literal["ignore", "nurture", "book_demo", "escalate"]
    follow_up: str

def qualify_lead(email: str) -> Lead | None:
    response = client.chat.completions.parse(
        model="gpt-4o",
        messages=[{
            "role": "user",
            "content": f"Qualify this sales lead.\\n\\nEmail: {email}",
        }],
        response_format=Lead,
    )
    lead = response.choices[0].message.parsed
    if lead and lead.next_action == "escalate" and not lead.signals:
        raise ValueError("missing evidence")
    return lead
`;

const TS_NATIVE = `import OpenAI from "openai";
import { zodResponseFormat } from "openai/helpers/zod";
import { z } from "zod";

const client = new OpenAI();

const Lead = z.object({
  contact: z.object({
    name: z.string(),
    email: z.string(),
    role: z.string(),
  }),
  company: z.object({
    name: z.string(),
    size: z.enum(["startup", "midmarket", "enterprise"]),
  }),
  signals: z.array(z.object({
    label: z.string(),
    score: z.number(),
    evidence: z.string(),
  })),
  next_action: z.enum(["ignore", "nurture", "book_demo", "escalate"]),
  follow_up: z.string(),
});

async function qualifyLead(email: string) {
  const r = await client.chat.completions.parse({
    model: "gpt-4o",
    messages: [{
      role: "user",
      content: \`Qualify this sales lead.\\n\\nEmail: \${email}\`,
    }],
    response_format: zodResponseFormat(Lead, "lead"),
  });
  const lead = r.choices[0].message.parsed;
  if (lead?.next_action === "escalate" && !lead.signals.length) {
    throw new Error("missing evidence");
  }
  return lead;
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
from pydantic import BaseModel

client = OpenAI()

class Triage(BaseModel):
    intent: Literal["refund", "bug", "upgrade", "question"]
    priority: Literal["low", "medium", "high"]

class State(TypedDict):
    ticket: str
    intent: str
    priority: str
    reply: str

def classify(state: State):
    response = client.chat.completions.parse(
        model="gpt-4o",
        messages=[{
            "role": "user",
            "content": f"Classify this ticket.\\n\\nTicket: {state['ticket']}",
        }],
        response_format=Triage,
    )
    triage = response.choices[0].message.parsed
    return {"intent": triage.intent, "priority": triage.priority}

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

type CmpRow = {
  aspect: string;
  standard: string;
  baml: string;
};

type Comparison = {
  id: string;
  tab: string;
  headline: string;
  body: string;
  bullets: string[];
  table: CmpRow[];
  baml: Sample;
  caller: Sample;
  native: Sample;
};

type TabMeta = {
  Icon?: IconType;
  Lucide?: typeof Sparkles;
  image?: string;
  brandColor: string;
};

const BAML_META: TabMeta = {
  image: '/bamllogopurple.svg',
  brandColor: '#7C3AED',
};

const TAB_META: Record<string, TabMeta> = {
  python: { image: '/python-icon.png', brandColor: '#3776AB' },
  typescript: { Icon: SiTypescript, brandColor: '#3178C6' },
  go: { Icon: SiGo, brandColor: '#00ADD8' },
  rust: { Icon: SiRust, brandColor: '#1A1612' },
  langgraph: { Lucide: Workflow, brandColor: '#1F8B4C' },
  'ai-sdk': { Lucide: Sparkles, brandColor: '#000000' },
};

const COMPARISONS: Comparison[] = [
  {
    id: 'python',
    tab: 'Python',
    headline: 'BAML vs Python.',
    body: 'Pydantic + the OpenAI SDK gets you typed outputs. The prompt still lives as a string in your app, the schema is duplicated per service, and the whole thing only works in Python. BAML lifts schema, prompt, and tests into one file you can call from anywhere.',
    bullets: [
      'Prompt and schema live in BAML, not buried in app code.',
      'Pydantic still works for everything else — BAML replaces only the LLM boundary.',
      'Tests sit next to the function. Run them with `baml test`.',
      'Same .baml file generates clients for TypeScript, Go, and Rust too.',
    ],
    table: [
      {
        aspect: 'Class definitions',
        standard: '4 nested Pydantic classes',
        baml: '1 flat class with @description hints',
      },
      {
        aspect: 'Where the prompt lives',
        standard: 'f-string inside messages[]',
        baml: 'prompt block — testable in isolation',
      },
      {
        aspect: 'Evidence requirement',
        standard: 'Hand-rolled `if/raise` after parse',
        baml: 'Stated in the prompt, enforced by the type',
      },
      {
        aspect: 'Reach',
        standard: 'Python-only — re-author for each service',
        baml: 'Same .baml generates TS / Go / Rust clients',
      },
    ],
    baml: { code: BAML_USER, lang: 'baml', filename: 'lead.baml' },
    caller: { code: PY_CALLER, lang: 'python', filename: 'app.py' },
    native: { code: PY_NATIVE, lang: 'python', filename: 'lead.py' },
  },
  {
    id: 'typescript',
    tab: 'TypeScript',
    headline: 'BAML vs TypeScript.',
    body: 'Zod + the OpenAI SDK gives you typed outputs in TypeScript. The schema and prompt still live in your route handler, the prompt is untestable in isolation, and you re-author it for every other service. BAML keeps the typed-output shape but moves the prompt, tests, and schema into one place.',
    bullets: [
      'BAML compiles to a typed TS client — the call site looks like any other function.',
      'Streams typed partials into your UI without bespoke JSON parsers.',
      'Keep Zod for everything else; use BAML at the LLM boundary.',
      'Provider swap is a one-line client change, not a SDK rewrite.',
    ],
    table: [
      {
        aspect: 'Schema syntax',
        standard: 'z.object({…}).enum().array() chain',
        baml: 'Declarative class with literal unions',
      },
      {
        aspect: 'Call site',
        standard: 'completions.parse + zodResponseFormat',
        baml: '`await b.QualifyLead(email)`',
      },
      {
        aspect: 'Streaming',
        standard: 'Switch to streamText, re-wire types',
        baml: 'Same function streams typed partials',
      },
      {
        aspect: 'Provider',
        standard: 'Hardcoded "gpt-4o" per route',
        baml: 'Swap models in one client block',
      },
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
    table: [
      {
        aspect: 'Struct boilerplate',
        standard: '4 structs + ~12 `json:"…"` tags',
        baml: '0 — generated from the .baml class',
      },
      {
        aspect: 'API call',
        standard: 'openai.NewClient + CreateChatCompletion',
        baml: '`b.QualifyLead(ctx, email)`',
      },
      {
        aspect: 'Decode',
        standard: 'json.Unmarshal — fails on fuzzy enums',
        baml: 'Schema-aware parse, fuzzy-tolerant',
      },
      {
        aspect: 'Error handling',
        standard: 'Two `if err != nil` branches',
        baml: 'One typed return value',
      },
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
    table: [
      {
        aspect: 'Type definitions',
        standard: '4 `#[derive(Deserialize)]` structs',
        baml: '1 generated struct from .baml class',
      },
      {
        aspect: 'HTTP layer',
        standard: 'reqwest::Client + bearer_auth + json!()',
        baml: '`b::qualify_lead(&ctx, email).await?`',
      },
      {
        aspect: 'Decode',
        standard: 'serde_json::from_str — no repair',
        baml: 'Schema-aware parse before Rust sees it',
      },
      {
        aspect: 'Repair / retry',
        standard: 'Hand-roll on top of anyhow',
        baml: 'Built into the parse boundary',
      },
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
    table: [
      {
        aspect: 'Wiring',
        standard: 'StateGraph + 3 nodes + edges + compile()',
        baml: 'One typed function — call it directly',
      },
      {
        aspect: 'State',
        standard: 'TypedDict shape passed between nodes',
        baml: 'Typed return value, no shared dict',
      },
      {
        aspect: 'Routing',
        standard: 'add_conditional_edges + Literal',
        baml: 'Tagged union + `match` over the type',
      },
      {
        aspect: 'Where to test',
        standard: 'Compile graph, drive with fixtures',
        baml: 'Call the function directly in `baml test`',
      },
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
    table: [
      {
        aspect: 'Where the schema lives',
        standard: 'Inline `z.object` per route handler',
        baml: 'Shared .baml class — imported as a type',
      },
      {
        aspect: 'Function shape',
        standard: '`generateObject({ model, schema, prompt })`',
        baml: '`await b.HandleTicket(ticket)`',
      },
      {
        aspect: 'Streaming vs. parsing',
        standard: 'Two APIs (streamText, generateObject)',
        baml: 'Same function, optionally streamed',
      },
      {
        aspect: 'Provider',
        standard: '`openai("gpt-4o")` per handler',
        baml: 'One client block — every function inherits',
      },
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

// ── Tab logo helper ─────────────────────────────────────────────────────────

function TabLogo({
  meta,
  size = 14,
  active = true,
}: {
  meta: TabMeta;
  size?: number;
  active?: boolean;
}) {
  const color = active ? meta.brandColor : '#8A8580';
  if (meta.image) {
    return (
      <Image
        alt=""
        aria-hidden
        height={size}
        src={meta.image}
        style={{
          filter: active ? 'none' : 'grayscale(1) opacity(0.7)',
          height: size,
          objectFit: 'contain',
          width: size,
        }}
        width={size}
      />
    );
  }
  if (meta.Icon) {
    return <meta.Icon size={size} style={{ color, flexShrink: 0 }} />;
  }
  if (meta.Lucide) {
    return <meta.Lucide size={size} style={{ color, flexShrink: 0 }} />;
  }
  return null;
}

// ── Code block (dark IDE style) ─────────────────────────────────────────────

function CodeBlock({
  tokens,
  filename,
  rawCode,
  langMeta,
}: {
  tokens: CodeTokens;
  filename: string;
  rawCode: string;
  langMeta?: TabMeta;
}) {
  const [copied, setCopied] = useState(false);
  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(rawCode);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* no-op */
    }
  };

  return (
    <div className="vs-code-window group">
      <div className="vs-code-titlebar">
        <div className="vs-code-dots">
          <span className="vs-code-dot" style={{ background: '#FF5F57' }} />
          <span className="vs-code-dot" style={{ background: '#FEBC2E' }} />
          <span className="vs-code-dot" style={{ background: '#28C840' }} />
        </div>
        <div className="vs-code-filename">
          {langMeta && <TabLogo meta={langMeta} size={12} />}
          <span>{filename}</span>
        </div>
        <button
          aria-label={copied ? 'Copied' : 'Copy code'}
          className="vs-copy-btn"
          onClick={onCopy}
          type="button"
        >
          {copied ? <Check size={13} /> : <Copy size={13} />}
        </button>
      </div>

      <div className="vs-code-body">
        <div aria-hidden className="vs-code-gutter">
          {tokens.map((_, i) => (
            <div key={`ln-${i}`}>{i + 1}</div>
          ))}
        </div>
        <pre className="vs-code-pre">
          <code>
            {tokens.map((line, i) => (
              <div className="vs-code-line" key={`l-${i}`}>
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

// ── IDE-style tab bar ───────────────────────────────────────────────────────

function VsTabs({
  activeId,
  onChange,
}: {
  activeId: string;
  onChange: (id: string) => void;
}) {
  return (
    <div aria-label="Comparison language" className="vs-tabbar" role="tablist">
      {COMPARISONS.flatMap((c, i) => {
        const isActive = c.id === activeId;
        const meta = TAB_META[c.id];
        const tabBtn = (
          <button
            aria-selected={isActive}
            className={`vs-tab${isActive ? ' vs-tab--active' : ''}`}
            key={c.id}
            onClick={() => onChange(c.id)}
            role="tab"
            style={
              isActive
                ? ({
                    ['--vs-brand' as any]: meta.brandColor,
                  } as React.CSSProperties)
                : undefined
            }
            type="button"
          >
            <TabLogo active={isActive} meta={meta} size={14} />
            <span>{c.tab}</span>
          </button>
        );
        return i > 0
          ? [
              <span
                aria-hidden
                className="vs-tabbar-sep"
                key={`sep-${c.id}`}
              />,
              tabBtn,
            ]
          : [tabBtn];
      })}
    </div>
  );
}

// ── Comparison table ────────────────────────────────────────────────────────

function ComparisonTable({
  rows,
  brandColor,
}: {
  rows: CmpRow[];
  brandColor: string;
}) {
  return (
    <div className="vs-table" role="table">
      <div className="vs-table-head" role="row">
        <div role="columnheader">Aspect</div>
        <div role="columnheader">Standard</div>
        <div role="columnheader" style={{ color: brandColor }}>
          BAML
        </div>
      </div>
      {rows.map((row) => (
        <div className="vs-table-row" key={row.aspect} role="row">
          <div className="vs-table-aspect" role="cell">
            {row.aspect}
          </div>
          <div className="vs-table-standard" role="cell">
            <span aria-hidden className="vs-table-mark vs-table-mark--neg">
              ✕
            </span>
            <span>{row.standard}</span>
          </div>
          <div
            className="vs-table-baml"
            role="cell"
            style={{ ['--vs-brand' as any]: brandColor } as React.CSSProperties}
          >
            <span aria-hidden className="vs-table-mark vs-table-mark--pos">
              <Check size={12} strokeWidth={2.4} />
            </span>
            <span>{row.baml}</span>
          </div>
        </div>
      ))}
    </div>
  );
}

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
  const meta = TAB_META[comparison.id];

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

        <div className="vs-code-grid">
          <div>
            <div className="vs-pane-label">{comparison.tab}</div>
            <CodeBlock
              filename={comparison.native.filename}
              langMeta={meta}
              rawCode={comparison.native.code}
              tokens={nativeTokens}
            />
          </div>
          <div>
            <div className="vs-pane-label vs-pane-label--baml">BAML</div>
            <div style={{ display: 'grid', gap: 14 }}>
              <CodeBlock
                filename={comparison.baml.filename}
                langMeta={BAML_META}
                rawCode={comparison.baml.code}
                tokens={bamlTokens}
              />
              <CodeBlock
                filename={comparison.caller.filename}
                langMeta={meta}
                rawCode={comparison.caller.code}
                tokens={bamlCallTokens}
              />
            </div>
          </div>
        </div>

        {/* <ComparisonTable
          brandColor={meta.brandColor}
          rows={comparison.table}
        /> */}
      </motion.div>
    </AnimatePresence>
  );
}

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

      <style>{`
        /* IDE-style tab bar */
        .vs-tabbar {
          align-items: stretch;
          backdrop-filter: blur(8px) saturate(140%);
          background: rgba(255, 255, 255, 0.65);
          border: 1px solid ${BORDER};
          border-radius: 10px;
          box-shadow:
            0 1px 0 rgba(255, 255, 255, 0.6) inset,
            0 1px 0 rgba(0, 0, 0, 0.02),
            0 12px 28px -22px rgba(26, 22, 18, 0.18);
          display: inline-flex;
          margin-top: 28px;
          max-width: 100%;
          overflow-x: auto;
          overflow-y: hidden;
          scrollbar-width: none;
        }
        .vs-tabbar::-webkit-scrollbar {
          display: none;
        }
        .vs-tabbar-sep {
          align-self: center;
          background: ${BORDER};
          height: 18px;
          width: 1px;
        }
        .vs-tab {
          align-items: center;
          background: transparent;
          border: none;
          border-bottom: 2px solid transparent;
          color: ${MUTED};
          cursor: pointer;
          display: inline-flex;
          font-family: ${HAND};
          font-size: 13.5px;
          font-weight: 500;
          gap: 8px;
          flex-shrink: 0;
          letter-spacing: 0.005em;
          padding: 12px 18px;
          white-space: nowrap;
          transition:
            background-color 200ms ease,
            color 200ms ease,
            border-color 200ms ease;
        }
        .vs-tab:hover {
          background: rgba(0, 0, 0, 0.02);
          color: ${INK};
        }
        .vs-tab--active,
        .vs-tab--active:hover {
          background: rgba(255, 255, 255, 0.95);
          border-bottom-color: var(--vs-brand);
          color: var(--vs-brand);
          font-weight: 600;
        }

        /* Code grid + pane labels */
        .vs-code-grid {
          display: grid;
          gap: 20px;
          grid-template-columns: 1fr 1fr;
          margin-top: 32px;
        }
        @media (max-width: 900px) {
          .vs-code-grid {
            grid-template-columns: 1fr;
          }
        }
        .vs-pane-label {
          color: #8A8580;
          font-family: ${MONO};
          font-size: 11px;
          font-weight: 500;
          letter-spacing: 0.12em;
          margin-bottom: 8px;
          text-transform: uppercase;
        }
        .vs-pane-label--baml {
          color: ${ACCENT};
        }

        /* Light code window */
        .vs-code-window {
          background: ${CODE_BG};
          border: 1px solid ${BORDER};
          border-radius: 10px;
          box-shadow:
            0 1px 0 rgba(255, 255, 255, 0.6) inset,
            0 18px 40px -28px rgba(26, 22, 18, 0.18);
          display: flex;
          flex-direction: column;
          height: 100%;
          overflow: hidden;
          position: relative;
          width: 100%;
        }
        .vs-code-titlebar {
          align-items: center;
          background: ${GUTTER_BG};
          border-bottom: 1px solid ${BORDER};
          color: ${MUTED};
          display: grid;
          font-family: ${MONO};
          font-size: 11px;
          gap: 8px;
          grid-template-columns: 60px 1fr 32px;
          height: 32px;
          padding: 0 12px;
          position: relative;
        }
        .vs-code-dots {
          align-items: center;
          display: flex;
          gap: 6px;
        }
        .vs-code-dot {
          border: 1px solid rgba(0, 0, 0, 0.06);
          border-radius: 50%;
          height: 10px;
          width: 10px;
        }
        .vs-code-filename {
          align-items: center;
          color: ${INK};
          display: inline-flex;
          gap: 6px;
          justify-content: center;
          letter-spacing: 0.04em;
        }
        .vs-copy-btn {
          align-items: center;
          background: rgba(255, 255, 255, 0.6);
          border: 1px solid transparent;
          border-radius: 6px;
          color: ${MUTED};
          cursor: pointer;
          display: inline-flex;
          height: 24px;
          justify-content: center;
          opacity: 0;
          padding: 0;
          transition:
            opacity 180ms ease,
            background-color 180ms ease,
            color 180ms ease,
            border-color 180ms ease;
          width: 24px;
        }
        .vs-code-window:hover .vs-copy-btn,
        .vs-copy-btn:focus-visible {
          opacity: 1;
        }
        .vs-copy-btn:hover {
          background: #ffffff;
          border-color: ${BORDER};
          color: ${INK};
        }
        .vs-code-body {
          background: ${CODE_BG};
          display: flex;
          flex: 1;
          font-family: ${MONO};
          font-size: 12.5px;
          line-height: 20px;
          min-height: 0;
        }
        .vs-code-gutter {
          background: ${GUTTER_BG};
          border-right: 1px solid ${BORDER};
          color: #B8B0A0;
          flex-shrink: 0;
          font-variant-numeric: tabular-nums;
          padding: 12px 8px;
          text-align: right;
          user-select: none;
          width: ${LINE_NUM_WIDTH}px;
        }
        .vs-code-pre {
          background: transparent;
          color: ${INK};
          flex: 1;
          margin: 0;
          min-width: 0;
          overflow-wrap: anywhere;
          padding: 12px 16px;
          tab-size: 4;
          white-space: pre-wrap;
          word-break: break-word;
        }
        .vs-code-line {
          min-height: 20px;
        }

        /* Comparison table */
        .vs-table {
          backdrop-filter: blur(10px) saturate(140%);
          background: rgba(255, 255, 255, 0.55);
          border: 1px solid ${BORDER};
          border-radius: 12px;
          box-shadow:
            0 1px 0 rgba(255, 255, 255, 0.6) inset,
            0 18px 40px -28px rgba(26, 22, 18, 0.18);
          margin-top: 36px;
          overflow: hidden;
        }
        .vs-table-head,
        .vs-table-row {
          display: grid;
          grid-template-columns: minmax(180px, 1fr) 1.4fr 1.4fr;
        }
        .vs-table-head {
          background: rgba(255, 255, 255, 0.7);
          border-bottom: 1px solid ${BORDER};
          color: #8A8580;
          font-family: ${MONO};
          font-size: 11px;
          font-weight: 600;
          letter-spacing: 0.12em;
          text-transform: uppercase;
        }
        .vs-table-head > div {
          padding: 12px 18px;
        }
        .vs-table-row {
          align-items: center;
          border-top: 1px solid rgba(217, 211, 196, 0.55);
          color: ${INK};
          font-family: ${HAND};
          font-size: 14.5px;
        }
        .vs-table-row:first-of-type {
          border-top: none;
        }
        .vs-table-row > div {
          padding: 14px 18px;
        }
        .vs-table-aspect {
          color: ${INK};
          font-weight: 600;
        }
        .vs-table-standard,
        .vs-table-baml {
          align-items: center;
          display: flex;
          gap: 10px;
        }
        .vs-table-standard {
          color: ${MUTED};
        }
        .vs-table-baml {
          color: var(--vs-brand);
          font-weight: 500;
        }
        .vs-table-mark {
          align-items: center;
          border-radius: 999px;
          display: inline-flex;
          flex-shrink: 0;
          font-family: ${MONO};
          font-size: 11px;
          font-weight: 700;
          height: 18px;
          justify-content: center;
          width: 18px;
        }
        .vs-table-mark--neg {
          background: rgba(180, 110, 110, 0.12);
          color: #B46E6E;
        }
        .vs-table-mark--pos {
          background: rgba(31, 139, 76, 0.14);
          color: #1F8B4C;
        }
        @media (max-width: 720px) {
          .vs-table-head,
          .vs-table-row {
            grid-template-columns: 1fr;
          }
          .vs-table-head > div:not(:first-child),
          .vs-table-row > div:not(:first-child) {
            border-top: 1px dashed rgba(217, 211, 196, 0.6);
          }
        }
      `}</style>
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
    command: 'claude plugin marketplace add BoundaryML/baml-skill',
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
          <div className="cta-left">
            <p className="cta-eyebrow">Install</p>
            <div className="install-card">
              <div className="plugin-tab-row">
                {installOptions.map((option) => {
                  const isActive = installPath === option.id;
                  return (
                    <button
                      className={`plugin-tab${isActive ? ' plugin-tab--active' : ''}`}
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
              <ScriptCopyBtn
                className="install-script"
                codeLanguage="bash"
                commandMap={{ bash: selected.command } as const}
                darkTheme="none"
                lightTheme="none"
                showMultiplePackageOptions={false}
              />
            </div>
            <div className="manual-install">
              <span className="manual-install__label">
                Prefer manual setup?
              </span>
              <a
                className="manual-install__link"
                href="https://docs.boundaryml.com/guide/installation-language/python"
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
              <a className="editorial-btn editorial-btn--ghost" href="/explore">
                Read the thesis
                <span aria-hidden>→</span>
              </a>
            </div>
          </div>
        </div>
      </div>

      <style>{`
        .cta-grid {
          align-items: center;
          display: grid;
          gap: 56px;
          grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
          margin-top: 48px;
        }
        .cta-left {
          align-items: flex-start;
          display: flex;
          flex-direction: column;
          gap: 14px;
        }
        .cta-eyebrow {
          color: #8A8580;
          font-family: ${HAND};
          font-size: 11px;
          font-weight: 600;
          letter-spacing: 0.16em;
          margin: 0;
          text-transform: uppercase;
        }
        .install-card {
          background: #ffffff;
          border: 1px solid ${BORDER};
          border-radius: 12px;
          box-shadow:
            0 1px 0 rgba(0, 0, 0, 0.02),
            0 18px 40px -28px rgba(26, 22, 18, 0.18);
          display: flex;
          flex-direction: column;
          max-width: 460px;
          overflow: hidden;
          width: 100%;
        }
        .plugin-tab-row {
          background: #FBF7EE;
          border-bottom: 1px solid ${BORDER};
          display: flex;
          gap: 4px;
          padding: 6px 6px 0;
        }
        .plugin-tab {
          align-items: center;
          background: transparent;
          border: 1px solid transparent;
          border-bottom: none;
          border-radius: 8px 8px 0 0;
          color: ${MUTED};
          cursor: pointer;
          display: inline-flex;
          font-family: ${HAND};
          font-size: 13px;
          font-weight: 500;
          gap: 6px;
          padding: 8px 14px;
          position: relative;
          top: 1px;
          transition: background-color 200ms ease, color 200ms ease, border-color 200ms ease;
        }
        .plugin-tab:hover {
          color: ${ACCENT};
        }
        .plugin-tab--active,
        .plugin-tab--active:hover {
          background: #ffffff;
          border-color: ${BORDER};
          color: ${INK};
        }
        .install-script {
          padding: 14px 16px 16px;
        }
        .install-script .max-w-lg {
          max-width: none;
        }
        .cta-right {
          align-items: flex-start;
          display: flex;
          flex-direction: column;
          gap: 22px;
        }
        .cta-row {
          align-items: center;
          display: flex;
          flex-wrap: wrap;
          gap: 12px;
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
          .install-card {
            max-width: none;
          }
          .plugin-tab {
            flex: 1;
            justify-content: center;
          }
        }
      `}</style>
    </section>
  );
}
