'use client';

import { BAML } from '@boundaryml/baml-lezer';
import { EditorView, lineNumbers } from '@codemirror/view';
import { vscodeLightInit } from '@uiw/codemirror-theme-vscode';
import CodeMirror from '@uiw/react-codemirror';
import { useReducedMotion } from 'motion/react';
import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';

const BG = '#ffffff';
const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';
const CODE_BG = '#FDFBF6';
const GUTTER_BG = '#F5F1E5';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';

const BAML_CODE = `enum RiskTier {
  low
  review
  block
}

class LineItem {
  sku         string?
  name        string
  quantity    int
  unit_price  float
  confidence  float @stream.done
}

class InvoiceDraft {
  vendor      string @stream.done
  currency    string
  due_date    string?
  total       float @stream.done
  line_items  LineItem[]
}

class ValidationIssue {
  path      string
  severity  "warn" | "error"
  message   string
}

class Invoice {
  vendor       string
  amount_due   float
  due_date     string?
  risk         RiskTier
  issues       ValidationIssue[]
}

class ParseFailure {
  reason string
  retryable bool
}

function ExtractInvoice(
  text: string,
  locale: string
) -> InvoiceDraft throws ParseFailure {
  client GPT4o
  prompt #"
    Extract a normalized invoice draft.
    Prefer ISO dates and preserve uncertain line items.
    {{ ctx.output_format }}

    Locale: {{ locale }}
    {{ _.role("user") }}
    {{ text }}
  "#
}

function ValidateInvoice(draft: InvoiceDraft) -> ValidationIssue[] {
  let line_total = draft.line_items
    .map((item) => item.quantity * item.unit_price)
    .sum();

  if ((line_total - draft.total).abs() > 0.02) {
    [ValidationIssue {
      path: "total",
      severity: "error",
      message: "line item sum does not match total",
    }]
  } else {
    []
  }
}

function RiskScore(draft: InvoiceDraft) -> RiskTier {
  if (draft.total > 25000.0) { RiskTier.block }
  else if (draft.currency != "USD") { RiskTier.review }
  else { RiskTier.low }
}

function ExtractAndReview(text: string) -> Invoice {
  let draft = ExtractInvoice(text, "en-US") catch (err) {
    _: ParseFailure => ExtractInvoice(text, "auto")
  };

  Invoice {
    vendor: draft.vendor.trim(),
    amount_due: draft.total,
    due_date: draft.due_date,
    risk: RiskScore(draft),
    issues: ValidateInvoice(draft),
  }
}

testset "invoice-review" {
  test "flags mismatched totals" {
    let invoice = ExtractAndReview("Acme invoice 1001...");
    assert.equal(invoice.risk, RiskTier.review);
    assert.contains(invoice.issues.map((i) => i.path), "total");
  }
}`;

// ── Code panel ────────────────────────────────────────────────────────────────

const codeMirrorTheme = vscodeLightInit({
  settings: {
    background: CODE_BG,
    fontFamily: MONO,
    fontSize: '13px',
    foreground: INK,
    gutterActiveForeground: MUTED,
    gutterBackground: GUTTER_BG,
    gutterBorder: BORDER,
    gutterForeground: '#B8B0A0',
    lineHighlight: 'transparent',
    selection: 'rgba(109,40,217,0.18)',
    selectionMatch: 'rgba(109,40,217,0.12)',
  },
});

const codeMirrorExtensions = [
  BAML(),
  lineNumbers(),
  EditorView.editable.of(false),
  EditorView.lineWrapping,
];

function CodePanel() {
  return (
    <div
      style={{
        background: CODE_BG,
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        width: '100%',
      }}
    >
      <div
        style={{
          alignItems: 'center',
          background: GUTTER_BG,
          borderBottom: `1px solid ${BORDER}`,
          color: MUTED,
          display: 'grid',
          flexShrink: 0,
          gridTemplateColumns: '60px 1fr 60px',
          height: 32,
          padding: '0 14px',
        }}
      >
        <div style={{ display: 'flex', gap: 6 }}>
          <span
            style={{
              background: '#E5A8A8',
              borderRadius: '50%',
              height: 10,
              width: 10,
            }}
          />
          <span
            style={{
              background: '#E5C58A',
              borderRadius: '50%',
              height: 10,
              width: 10,
            }}
          />
          <span
            style={{
              background: '#A8D0A0',
              borderRadius: '50%',
              height: 10,
              width: 10,
            }}
          />
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
          invoice.baml
        </span>
        <span />
      </div>

      <div
        style={{
          background: CODE_BG,
          flex: 1,
          minHeight: 0,
          overflow: 'auto',
        }}
      >
        <CodeMirror
          basicSetup={false}
          editable={false}
          extensions={codeMirrorExtensions}
          height="100%"
          readOnly
          style={{ background: CODE_BG, fontFamily: MONO, height: '100%' }}
          theme={codeMirrorTheme}
          value={BAML_CODE}
        />
      </div>
    </div>
  );
}

// ── Graph panel (stylized BAML workflow) ──────────────────────────────────────

type NodeState = 'idle' | 'running' | 'success';

type GraphNode = {
  id: string;
  kind: 'input' | 'llm' | 'class' | 'function' | 'output';
  label: string;
  client?: string;
  state: NodeState;
  x: number;
  y: number;
  w: number;
  h: number;
};

type GraphEdge = {
  from: string;
  to: string;
  active?: boolean;
};

// Single horizontal flow keeps the eye moving left-to-right and gives
// ExtractInvoice room to breathe as the visual centerpiece.
const VIEW_W = 760;
const VIEW_H = 360;

const GRAPH_NODES: GraphNode[] = [
  {
    id: 'input',
    kind: 'input',
    label: 'text',
    state: 'success',
    x: 24,
    y: 154,
    w: 100,
    h: 52,
  },
  {
    id: 'extract',
    kind: 'llm',
    label: 'ExtractInvoice',
    client: 'GPT-4o',
    state: 'running',
    x: 156,
    y: 70,
    w: 220,
    h: 220,
  },
  {
    id: 'draft',
    kind: 'class',
    label: 'InvoiceDraft',
    state: 'running',
    x: 400,
    y: 76,
    w: 196,
    h: 208,
  },
  {
    id: 'review',
    kind: 'function',
    label: 'ExtractAndReview',
    state: 'idle',
    x: 620,
    y: 154,
    w: 128,
    h: 52,
  },
];

const GRAPH_EDGES: GraphEdge[] = [
  { from: 'input', to: 'extract', active: true },
  { from: 'extract', to: 'draft', active: true },
  { from: 'draft', to: 'review' },
];

// Editorial palette specific to the graph panel — pulled from the same token
// family as the surrounding sections (cream paper, warm border, accent purple,
// olive success, github-light syntax for the JSON previews).
const G = {
  paperTop: '#FDFBF6',
  paperBottom: '#F5F1E5',
  grid: 'rgba(26,22,18,0.07)',
  cardTop: '#FFFFFF',
  cardBottom: '#FAF6EC',
  border: BORDER,
  ink: INK,
  muted: MUTED,
  eyebrow: EYEBROW,
  accent: '#7C3AED',
  accentSoft: 'rgba(124,58,237,0.10)',
  accentRing: 'rgba(124,58,237,0.32)',
  accentHalo: 'rgba(124,58,237,0.20)',
  success: '#4D7C0F',
  successSoft: 'rgba(77,124,15,0.10)',
  successRing: 'rgba(77,124,15,0.30)',
  classFg: '#0550AE',
  classSoft: 'rgba(5,80,174,0.08)',
};

const STATE_TINT: Record<NodeState, { ring: string; halo: string }> = {
  idle: {
    ring: G.border,
    halo: '0 0 0 1px rgba(26,22,18,0.04)',
  },
  running: {
    ring: G.accentRing,
    halo: `0 0 0 1px ${G.accentRing}, 0 0 32px ${G.accentHalo}`,
  },
  success: {
    ring: G.successRing,
    halo: `0 0 0 1px ${G.successRing}`,
  },
};

function StateIcon({
  state,
  size = 12,
  idleColor = 'rgba(255,255,255,0.85)',
}: {
  state: NodeState;
  size?: number;
  idleColor?: string;
}) {
  if (state === 'running') {
    return (
      <svg
        aria-hidden
        fill="none"
        height={size}
        stroke="white"
        strokeWidth="3"
        style={{
          animation: 'baml-perspective-spin 900ms linear infinite',
          transformOrigin: 'center',
        }}
        viewBox="0 0 24 24"
        width={size}
      >
        <circle cx="12" cy="12" r="10" opacity="0.25" />
        <path d="M4 12a8 8 0 018-8" strokeLinecap="round" />
      </svg>
    );
  }
  if (state === 'success') {
    return (
      <svg
        aria-hidden
        fill="none"
        height={size}
        stroke="white"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="3.5"
        viewBox="0 0 24 24"
        width={size}
      >
        <path d="M5 13l4 4L19 7" />
      </svg>
    );
  }
  return (
    <svg aria-hidden fill="none" height={size} viewBox="0 0 24 24" width={size}>
      <circle cx="12" cy="12" r="3" fill={idleColor} />
    </svg>
  );
}

function StateChip({ state, accent }: { state: NodeState; accent: string }) {
  const isIdle = state === 'idle';
  return (
    <span
      style={{
        alignItems: 'center',
        background: isIdle ? '#F0EBDB' : accent,
        border: isIdle ? `1px solid ${G.border}` : 'none',
        borderRadius: 6,
        boxShadow: isIdle
          ? 'none'
          : 'inset 0 1px 0 rgba(255,255,255,0.28), 0 1px 2px rgba(26,22,18,0.18)',
        color: isIdle ? G.eyebrow : '#FFFFFF',
        display: 'inline-flex',
        flexShrink: 0,
        height: 20,
        justifyContent: 'center',
        width: 20,
      }}
    >
      <StateIcon state={state} idleColor={G.eyebrow} />
    </span>
  );
}

function chipForKind(kind: GraphNode['kind']): {
  label: string;
  fg: string;
  bg: string;
} {
  if (kind === 'llm') {
    return {
      label: 'LLM',
      fg: G.accent,
      bg: G.accentSoft,
    };
  }
  if (kind === 'class') {
    return {
      label: 'class',
      fg: G.classFg,
      bg: G.classSoft,
    };
  }
  if (kind === 'function') {
    return {
      label: 'fn',
      fg: G.success,
      bg: G.successSoft,
    };
  }
  return {
    label: kind === 'input' ? 'arg' : 'out',
    fg: G.muted,
    bg: 'rgba(138,133,128,0.10)',
  };
}

// Tokens for the streaming JSON preview inside the LLM node — github-light.
type Tok = { t: string; c?: string };
const COL = {
  key: '#8250DF',
  str: '#0A3069',
  num: '#0550AE',
  punct: '#6E7781',
  brace: '#1F2328',
};
const STREAM_LINES: Tok[][] = [
  [{ t: '{', c: COL.brace }],
  [
    { t: '  ' },
    { t: '"vendor"', c: COL.key },
    { t: ': ', c: COL.punct },
    { t: '"Acme Corp"', c: COL.str },
    { t: ',', c: COL.punct },
  ],
  [
    { t: '  ' },
    { t: '"total"', c: COL.key },
    { t: ': ', c: COL.punct },
    { t: '1247.50', c: COL.num },
    { t: ',', c: COL.punct },
  ],
  [
    { t: '  ' },
    { t: '"due_date"', c: COL.key },
    { t: ': ', c: COL.punct },
    { t: '"2026-06-▌"', c: COL.str },
  ],
];

function NodeShell({
  n,
  children,
}: {
  n: GraphNode;
  children: React.ReactNode;
}) {
  const tint = STATE_TINT[n.state];
  return (
    <div
      style={{
        background: `linear-gradient(180deg, ${G.cardTop} 0%, ${G.cardBottom} 100%)`,
        border: `1px solid ${tint.ring}`,
        borderRadius: 12,
        boxShadow: `${tint.halo}, 0 1px 0 rgba(26,22,18,0.04), 0 12px 28px -16px rgba(26,22,18,0.22), inset 0 1px 0 rgba(255,255,255,0.6)`,
        boxSizing: 'border-box',
        color: G.ink,
        display: 'flex',
        flexDirection: 'column',
        fontFamily:
          'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
        height: n.h,
        left: n.x,
        overflow: 'hidden',
        position: 'absolute',
        top: n.y,
        width: n.w,
      }}
    >
      {children}
    </div>
  );
}

function NodeHeader({ n, accent }: { n: GraphNode; accent: string }) {
  const chip = chipForKind(n.kind);
  return (
    <div
      style={{
        alignItems: 'center',
        display: 'flex',
        gap: 10,
        padding: '10px 12px',
      }}
    >
      <StateChip accent={accent} state={n.state} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            alignItems: 'center',
            display: 'flex',
            gap: 6,
          }}
        >
          <span
            style={{
              background: chip.bg,
              borderRadius: 4,
              boxShadow: `inset 0 0 0 1px ${chip.fg}33`,
              color: chip.fg,
              fontFamily: MONO,
              fontSize: 9,
              fontWeight: 700,
              letterSpacing: '0.08em',
              padding: '2px 5px',
              textTransform: 'uppercase',
            }}
          >
            {chip.label}
          </span>
          {n.client ? (
            <span
              style={{
                color: G.muted,
                fontFamily: MONO,
                fontSize: 10,
              }}
            >
              {n.client}
            </span>
          ) : null}
        </div>
        <div
          style={{
            color: G.ink,
            fontSize: 13,
            fontWeight: 600,
            letterSpacing: '-0.01em',
            lineHeight: 1.2,
            marginTop: 4,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {n.label}
        </div>
      </div>
    </div>
  );
}

function StreamingPreview() {
  return (
    <div
      style={{
        background: CODE_BG,
        border: `1px solid ${G.border}`,
        borderRadius: 8,
        flex: 1,
        margin: '0 12px 12px',
        minHeight: 0,
        overflow: 'hidden',
        padding: '10px 12px',
      }}
    >
      <div
        style={{
          alignItems: 'center',
          color: G.accent,
          display: 'flex',
          fontFamily: MONO,
          fontSize: 9,
          fontWeight: 700,
          gap: 6,
          letterSpacing: '0.12em',
          marginBottom: 6,
          textTransform: 'uppercase',
        }}
      >
        <span
          style={{
            background: G.accent,
            borderRadius: '50%',
            display: 'inline-block',
            height: 5,
            width: 5,
          }}
        />
        streaming
      </div>
      <pre
        style={{
          color: G.ink,
          fontFamily: MONO,
          fontSize: 10.5,
          lineHeight: 1.5,
          margin: 0,
          whiteSpace: 'pre',
        }}
      >
        {STREAM_LINES.map((line, i) => (
          <div key={`stream-${i}`}>
            {line.map((tok, j) => (
              <span key={`stream-${i}-${j}`} style={{ color: tok.c }}>
                {tok.t}
              </span>
            ))}
          </div>
        ))}
      </pre>
    </div>
  );
}

function ResultPreview() {
  type Row =
    | { k: string; v: string; vc: string; status: 'done' }
    | { k: string; v: string; vc: string; status: 'partial' }
    | { k: string; status: 'pending' };
  const rows: Row[] = [
    { k: 'vendor', v: '"Acme Corp"', vc: COL.str, status: 'done' },
    { k: 'total', v: '1247.50', vc: COL.num, status: 'done' },
    { k: 'due_date', v: '"2026-06-▌"', vc: COL.str, status: 'partial' },
    { k: 'line_items', status: 'pending' },
  ];
  return (
    <div
      style={{
        background: CODE_BG,
        border: `1px solid ${G.border}`,
        borderRadius: 8,
        flex: 1,
        margin: '0 12px 12px',
        minHeight: 0,
        overflow: 'hidden',
        padding: '10px 12px',
      }}
    >
      <div
        style={{
          alignItems: 'center',
          color: G.accent,
          display: 'flex',
          fontFamily: MONO,
          fontSize: 9,
          fontWeight: 700,
          gap: 6,
          letterSpacing: '0.12em',
          marginBottom: 6,
          textTransform: 'uppercase',
        }}
      >
        <span
          style={{
            background: G.accent,
            borderRadius: '50%',
            display: 'inline-block',
            height: 5,
            width: 5,
          }}
        />
        InvoiceDraft · partial
      </div>
      <div style={{ display: 'grid', rowGap: 4 }}>
        {rows.map((r) => (
          <div
            key={r.k}
            style={{
              alignItems: 'baseline',
              display: 'grid',
              fontFamily: MONO,
              fontSize: 10.5,
              gap: 8,
              gridTemplateColumns: '78px 1fr',
              lineHeight: 1.4,
              opacity: r.status === 'pending' ? 0.7 : 1,
            }}
          >
            <span style={{ color: COL.key }}>{r.k}</span>
            {r.status === 'pending' ? (
              <span
                style={{
                  alignItems: 'center',
                  color: G.eyebrow,
                  display: 'inline-flex',
                  gap: 6,
                }}
              >
                <span
                  style={{
                    background:
                      'repeating-linear-gradient(90deg, rgba(26,22,18,0.18) 0 6px, transparent 6px 10px)',
                    borderRadius: 2,
                    display: 'inline-block',
                    height: 7,
                    width: 56,
                  }}
                />
                <span style={{ fontSize: 9, opacity: 0.85 }}>pending</span>
              </span>
            ) : (
              <span style={{ color: r.vc }}>{r.v}</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function GraphPanel({ runningPulse }: { runningPulse: boolean }) {
  const nodeById: Record<string, GraphNode> = Object.fromEntries(
    GRAPH_NODES.map((n) => [n.id, n]),
  );

  return (
    <div
      style={{
        background: `linear-gradient(180deg, ${G.paperTop} 0%, ${G.paperBottom} 100%)`,
        color: G.ink,
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        width: '100%',
      }}
    >
      {/* top chrome */}
      <div
        style={{
          alignItems: 'center',
          background: GUTTER_BG,
          borderBottom: `1px solid ${G.border}`,
          color: G.muted,
          display: 'grid',
          flexShrink: 0,
          gridTemplateColumns: '80px 1fr 80px',
          height: 32,
          padding: '0 14px',
        }}
      >
        <div style={{ display: 'flex', gap: 6 }}>
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
            color: G.muted,
            fontFamily: MONO,
            fontSize: 11,
            letterSpacing: '0.04em',
            textAlign: 'center',
          }}
        >
          Pipeline · graph view
        </span>
        <span
          style={{
            alignItems: 'center',
            color: G.accent,
            display: 'inline-flex',
            fontFamily: MONO,
            fontSize: 10,
            fontWeight: 600,
            gap: 6,
            justifyContent: 'flex-end',
            letterSpacing: '0.04em',
            textTransform: 'uppercase',
          }}
        >
          <span
            style={{
              background: G.accent,
              borderRadius: '50%',
              boxShadow: runningPulse ? `0 0 0 4px ${G.accentHalo}` : 'none',
              display: 'inline-block',
              height: 6,
              width: 6,
            }}
          />
          running
        </span>
      </div>

      {/* canvas */}
      <div
        style={{
          backgroundImage: `radial-gradient(${G.grid} 1px, transparent 1px)`,
          backgroundSize: '22px 22px',
          flex: 1,
          minHeight: 0,
          overflow: 'hidden',
          position: 'relative',
          width: '100%',
        }}
      >
        <div
          style={{
            display: 'flex',
            height: '100%',
            justifyContent: 'center',
            padding: '20px 24px',
            width: '100%',
          }}
        >
          <div
            style={{
              aspectRatio: `${VIEW_W} / ${VIEW_H}`,
              maxHeight: '100%',
              maxWidth: '100%',
              position: 'relative',
              width: '100%',
            }}
          >
            <svg
              aria-hidden
              preserveAspectRatio="xMidYMid meet"
              style={{
                inset: 0,
                position: 'absolute',
                width: '100%',
                height: '100%',
              }}
              viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
            >
              <defs>
                <marker
                  id="ps-arrow"
                  markerHeight="6"
                  markerUnits="strokeWidth"
                  markerWidth="6"
                  orient="auto"
                  refX="5"
                  refY="3"
                  viewBox="0 0 6 6"
                >
                  <path d="M0,0 L6,3 L0,6 z" fill="rgba(26,22,18,0.35)" />
                </marker>
                <marker
                  id="ps-arrow-active"
                  markerHeight="6"
                  markerUnits="strokeWidth"
                  markerWidth="6"
                  orient="auto"
                  refX="5"
                  refY="3"
                  viewBox="0 0 6 6"
                >
                  <path d="M0,0 L6,3 L0,6 z" fill={G.accent} />
                </marker>
              </defs>

              {GRAPH_EDGES.map((e, i) => {
                const a = nodeById[e.from];
                const b = nodeById[e.to];
                const x1 = a.x + a.w;
                const y1 = a.y + a.h / 2;
                const x2 = b.x;
                const y2 = b.y + b.h / 2;
                const dx = Math.max(40, (x2 - x1) * 0.55);
                const d = `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
                const color = e.active ? G.accent : 'rgba(26,22,18,0.30)';
                return (
                  <g key={`edge-${i}`}>
                    {e.active ? (
                      <path
                        d={d}
                        fill="none"
                        opacity="0.30"
                        stroke={G.accent}
                        strokeLinecap="round"
                        strokeWidth={6}
                        style={{ filter: 'blur(3px)' }}
                      />
                    ) : null}
                    <path
                      d={d}
                      fill="none"
                      markerEnd={`url(#${e.active ? 'ps-arrow-active' : 'ps-arrow'})`}
                      stroke={color}
                      strokeDasharray={e.active ? '6 4' : undefined}
                      strokeLinecap="round"
                      strokeWidth={e.active ? 1.6 : 1.2}
                      style={
                        e.active && runningPulse
                          ? {
                              animation:
                                'baml-perspective-dash 1.4s linear infinite',
                            }
                          : undefined
                      }
                    />
                  </g>
                );
              })}
            </svg>

            {/* nodes */}
            <svg
              preserveAspectRatio="xMidYMid meet"
              style={{
                height: '100%',
                inset: 0,
                position: 'absolute',
                width: '100%',
              }}
              viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
            >
              <foreignObject height={VIEW_H} width={VIEW_W} x={0} y={0}>
                <div
                  style={{
                    height: VIEW_H,
                    position: 'relative',
                    width: VIEW_W,
                  }}
                  {...({ xmlns: 'http://www.w3.org/1999/xhtml' } as any)}
                >
                  {GRAPH_NODES.map((n) => {
                    const accent =
                      n.state === 'running'
                        ? G.accent
                        : n.state === 'success'
                          ? G.success
                          : G.eyebrow;

                    if (n.kind === 'llm') {
                      return (
                        <NodeShell key={n.id} n={n}>
                          <NodeHeader accent={accent} n={n} />
                          <StreamingPreview />
                        </NodeShell>
                      );
                    }
                    if (n.kind === 'class') {
                      return (
                        <NodeShell key={n.id} n={n}>
                          <NodeHeader accent={accent} n={n} />
                          <ResultPreview />
                        </NodeShell>
                      );
                    }
                    // compact pill nodes for input / function / output
                    return (
                      <NodeShell key={n.id} n={n}>
                        <div
                          style={{
                            alignItems: 'center',
                            display: 'flex',
                            gap: 10,
                            height: '100%',
                            padding: '0 12px',
                          }}
                        >
                          <StateChip accent={accent} state={n.state} />
                          <div style={{ minWidth: 0 }}>
                            <div
                              style={{
                                color: G.eyebrow,
                                fontFamily: MONO,
                                fontSize: 9,
                                fontWeight: 700,
                                letterSpacing: '0.10em',
                                textTransform: 'uppercase',
                              }}
                            >
                              {chipForKind(n.kind).label}
                            </div>
                            <div
                              style={{
                                color: G.ink,
                                fontSize: 12.5,
                                fontWeight: 600,
                                letterSpacing: '-0.005em',
                                lineHeight: 1.2,
                                overflow: 'hidden',
                                textOverflow: 'ellipsis',
                                whiteSpace: 'nowrap',
                              }}
                            >
                              {n.label}
                            </div>
                          </div>
                        </div>
                      </NodeShell>
                    );
                  })}
                </div>
              </foreignObject>
            </svg>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Slider component ──────────────────────────────────────────────────────────

const HANDLE_SIZE = 44;

export function CompactPerspectivePanel() {
  const [pos, setPos] = useState(60);
  const [dragging, setDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const reduced = useReducedMotion();

  const updateFromClientX = useCallback((clientX: number) => {
    const el = containerRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const next = ((clientX - rect.left) / rect.width) * 100;
    setPos(Math.max(0, Math.min(100, next)));
  }, []);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    (e.target as Element).setPointerCapture?.(e.pointerId);
    setDragging(true);
    updateFromClientX(e.clientX);
  };

  useEffect(() => {
    if (!dragging) return;
    const move = (e: PointerEvent) => updateFromClientX(e.clientX);
    const up = () => setDragging(false);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', up);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', up);
    };
  }, [dragging, updateFromClientX]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 10 : 4;
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      setPos((p) => Math.max(0, p - step));
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      setPos((p) => Math.min(100, p + step));
    } else if (e.key === 'Home') {
      e.preventDefault();
      setPos(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      setPos(100);
    }
  };

  return (
    <div
      style={{
        background: CARD_BG,
        border: `1px solid ${BORDER}`,
        borderRadius: 0,
        height: '100%',
        overflow: 'hidden',
        padding: 10,
        position: 'relative',
        width: '100%',
      }}
    >
      <style>{`
        @keyframes baml-perspective-dash {
          to { stroke-dashoffset: -20; }
        }
        @keyframes baml-perspective-spin {
          to { transform: rotate(360deg); }
        }
      `}</style>
      <div
        aria-hidden
        style={{
          alignItems: 'center',
          color: EYEBROW,
          display: 'flex',
          fontFamily: MONO,
          fontSize: 10,
          fontWeight: 600,
          justifyContent: 'space-between',
          letterSpacing: '0.12em',
          marginBottom: 10,
          padding: '0 4px',
          textTransform: 'uppercase',
        }}
      >
        <span style={{ alignItems: 'center', display: 'flex', gap: 8 }}>
          <span
            aria-hidden
            style={{
              background: ACCENT,
              borderRadius: '50%',
              height: 6,
              width: 6,
            }}
          />
          what your agent sees
        </span>
        <span style={{ alignItems: 'center', display: 'flex', gap: 8 }}>
          what you see
          <span
            aria-hidden
            style={{
              background: INK,
              borderRadius: '50%',
              height: 6,
              width: 6,
            }}
          />
        </span>
      </div>

      <div
        ref={containerRef}
        role="presentation"
        style={{
          borderRadius: 8,
          height: 'calc(100% - 26px)',
          minHeight: 0,
          overflow: 'hidden',
          position: 'relative',
          touchAction: 'none',
          userSelect: 'none',
          width: '100%',
        }}
      >
        <div style={{ inset: 0, position: 'absolute' }}>
          <GraphPanel runningPulse={!reduced} />
        </div>
        <div
          style={{
            clipPath: `inset(0 ${100 - pos}% 0 0)`,
            inset: 0,
            position: 'absolute',
            transition: dragging || reduced ? 'none' : 'clip-path 80ms linear',
            willChange: 'clip-path',
          }}
        >
          <CodePanel />
        </div>
        <div
          aria-hidden
          style={{
            background:
              'linear-gradient(180deg, rgba(255,255,255,0.95) 0%, rgba(255,255,255,0.6) 100%)',
            boxShadow:
              '0 0 0 1px rgba(26,22,18,0.18), 0 0 24px rgba(109,40,217,0.25)',
            bottom: 0,
            left: `${pos}%`,
            pointerEvents: 'none',
            position: 'absolute',
            top: 0,
            transform: 'translateX(-50%)',
            transition: dragging || reduced ? 'none' : 'left 80ms linear',
            width: 2,
          }}
        />
        <div
          aria-label="Slide between graph view and source code"
          aria-orientation="horizontal"
          aria-valuemax={100}
          aria-valuemin={0}
          aria-valuenow={Math.round(pos)}
          onKeyDown={onKeyDown}
          onPointerDown={onPointerDown}
          role="slider"
          tabIndex={0}
          style={{
            alignItems: 'center',
            background: BG,
            border: `2px solid ${ACCENT}`,
            borderRadius: '50%',
            boxShadow:
              '0 6px 18px rgba(26,22,18,0.25), 0 0 0 6px rgba(109,40,217,0.12)',
            cursor: dragging ? 'grabbing' : 'grab',
            display: 'flex',
            height: HANDLE_SIZE,
            justifyContent: 'center',
            left: `${pos}%`,
            outlineOffset: 4,
            position: 'absolute',
            top: '50%',
            touchAction: 'none',
            transform: 'translate(-50%, -50%)',
            transition: dragging || reduced ? 'none' : 'left 80ms linear',
            width: HANDLE_SIZE,
          }}
        >
          <svg
            aria-hidden
            fill="none"
            height={18}
            stroke={ACCENT}
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2.4}
            viewBox="0 0 24 24"
            width={18}
          >
            <path d="M9 6 L4 12 L9 18" />
            <path d="M15 6 L20 12 L15 18" />
          </svg>
        </div>
      </div>
    </div>
  );
}

export function PerspectiveSlider() {
  const [pos, setPos] = useState(60);
  const [dragging, setDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const reduced = useReducedMotion();

  const updateFromClientX = useCallback((clientX: number) => {
    const el = containerRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const next = ((clientX - rect.left) / rect.width) * 100;
    setPos(Math.max(0, Math.min(100, next)));
  }, []);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    (e.target as Element).setPointerCapture?.(e.pointerId);
    setDragging(true);
    updateFromClientX(e.clientX);
  };

  useEffect(() => {
    if (!dragging) return;
    const move = (e: PointerEvent) => updateFromClientX(e.clientX);
    const up = () => setDragging(false);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', up);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', up);
    };
  }, [dragging, updateFromClientX]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 10 : 4;
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      setPos((p) => Math.max(0, p - step));
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      setPos((p) => Math.min(100, p + step));
    } else if (e.key === 'Home') {
      e.preventDefault();
      setPos(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      setPos(100);
    }
  };

  const sliderStyle: CSSProperties = {
    cursor: dragging ? 'grabbing' : 'ew-resize',
  };

  return (
    <section
      aria-label="What you see vs what your agent sees"
      style={{
        background: BG,
        borderTop: `1px solid ${BORDER}`,
        color: INK,
        padding: '120px 0 140px',
        width: '100%',
      }}
    >
      <style>{`
        @keyframes baml-perspective-dash {
          to { stroke-dashoffset: -20; }
        }
        @keyframes baml-perspective-spin {
          to { transform: rotate(360deg); }
        }
        @keyframes baml-perspective-blink {
          0%, 60% { opacity: 1; }
          61%, 100% { opacity: 0; }
        }
      `}</style>
      <div
        style={{
          margin: '0 auto',
          maxWidth: 1200,
          padding: '0 32px',
        }}
      >
        <div style={{ marginBottom: 56, textAlign: 'center' }}>
          <p
            style={{
              color: EYEBROW,
              fontSize: 13,
              fontWeight: 500,
              letterSpacing: '0.12em',
              margin: 0,
              textTransform: 'uppercase',
            }}
          >
            Same program. Two views.
          </p>
          <h2
            style={{
              color: INK,
              fontSize: 'clamp(2rem, 4vw, 3.25rem)',
              fontWeight: 600,
              letterSpacing: '-0.025em',
              lineHeight: 1.08,
              margin: '14px 0 0',
            }}
          >
            What you see —{' '}
            <span
              style={{
                color: ACCENT,
                fontFamily: 'var(--font-serif)',
                fontStyle: 'italic',
                fontWeight: 500,
              }}
            >
              what your agent sees
            </span>
            .
          </h2>
          <p
            style={{
              color: MUTED,
              fontSize: 17,
              lineHeight: 1.6,
              margin: '20px auto 0',
              maxWidth: 620,
            }}
          >
            BAML is the contract both sides read. Your agent gets typed source
            it can navigate, refactor, and test. You get the same program as an
            executable graph. Drag the slider to switch perspectives.
          </p>
        </div>

        <div
          style={{
            background: CARD_BG,
            border: `1px solid ${BORDER}`,
            borderRadius: 16,
            boxShadow:
              '0 1px 0 rgba(0,0,0,0.02), 0 24px 60px -32px rgba(26,22,18,0.25)',
            overflow: 'hidden',
            padding: 12,
            position: 'relative',
          }}
        >
          {/* labels above the panes */}
          <div
            aria-hidden
            style={{
              alignItems: 'center',
              color: EYEBROW,
              display: 'flex',
              fontFamily: MONO,
              fontSize: 11,
              fontWeight: 600,
              justifyContent: 'space-between',
              letterSpacing: '0.14em',
              marginBottom: 10,
              padding: '0 6px',
              textTransform: 'uppercase',
            }}
          >
            <span style={{ alignItems: 'center', display: 'flex', gap: 8 }}>
              <span
                aria-hidden
                style={{
                  background: ACCENT,
                  borderRadius: '50%',
                  height: 6,
                  width: 6,
                }}
              />
              what your agent sees
            </span>
            <span style={{ alignItems: 'center', display: 'flex', gap: 8 }}>
              what you see
              <span
                aria-hidden
                style={{
                  background: INK,
                  borderRadius: '50%',
                  height: 6,
                  width: 6,
                }}
              />
            </span>
          </div>

          <div
            ref={containerRef}
            role="presentation"
            style={{
              ...sliderStyle,
              borderRadius: 10,
              height: 'min(560px, 70vh)',
              minHeight: 360,
              overflow: 'hidden',
              position: 'relative',
              touchAction: 'none',
              userSelect: 'none',
              width: '100%',
            }}
          >
            {/* Layer 1: graph panel (background - what you see) */}
            <div
              style={{
                inset: 0,
                position: 'absolute',
              }}
            >
              <GraphPanel runningPulse={!reduced} />
            </div>

            {/* Layer 2: code panel (foreground - clipped to left of slider) */}
            <div
              style={{
                clipPath: `inset(0 ${100 - pos}% 0 0)`,
                inset: 0,
                position: 'absolute',
                transition:
                  dragging || reduced ? 'none' : 'clip-path 80ms linear',
                willChange: 'clip-path',
              }}
            >
              <CodePanel />
            </div>

            {/* divider line */}
            <div
              aria-hidden
              style={{
                background:
                  'linear-gradient(180deg, rgba(255,255,255,0.95) 0%, rgba(255,255,255,0.6) 100%)',
                boxShadow:
                  '0 0 0 1px rgba(26,22,18,0.18), 0 0 24px rgba(109,40,217,0.25)',
                left: `${pos}%`,
                pointerEvents: 'none',
                position: 'absolute',
                top: 0,
                bottom: 0,
                transform: 'translateX(-50%)',
                transition: dragging || reduced ? 'none' : 'left 80ms linear',
                width: 2,
              }}
            />

            {/* handle */}
            <div
              aria-label="Slide between graph view and source code"
              aria-orientation="horizontal"
              aria-valuemax={100}
              aria-valuemin={0}
              aria-valuenow={Math.round(pos)}
              onKeyDown={onKeyDown}
              onPointerDown={onPointerDown}
              role="slider"
              tabIndex={0}
              style={{
                alignItems: 'center',
                background: BG,
                border: `2px solid ${ACCENT}`,
                borderRadius: '50%',
                boxShadow:
                  '0 6px 18px rgba(26,22,18,0.25), 0 0 0 6px rgba(109,40,217,0.12)',
                cursor: dragging ? 'grabbing' : 'grab',
                display: 'flex',
                height: HANDLE_SIZE,
                justifyContent: 'center',
                left: `${pos}%`,
                outlineOffset: 4,
                position: 'absolute',
                top: '50%',
                touchAction: 'none',
                transform: 'translate(-50%, -50%)',
                transition: dragging || reduced ? 'none' : 'left 80ms linear',
                width: HANDLE_SIZE,
              }}
            >
              <svg
                aria-hidden
                fill="none"
                height={18}
                stroke={ACCENT}
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2.4}
                viewBox="0 0 24 24"
                width={18}
              >
                <path d="M9 6 L4 12 L9 18" />
                <path d="M15 6 L20 12 L15 18" />
              </svg>
            </div>
          </div>

          {/* footer hint */}
          <div
            style={{
              alignItems: 'center',
              color: MUTED,
              display: 'flex',
              fontFamily: MONO,
              fontSize: 11,
              gap: 12,
              justifyContent: 'center',
              letterSpacing: '0.04em',
              marginTop: 14,
              padding: '0 6px',
            }}
          >
            <span
              aria-hidden
              style={{
                background: BORDER,
                flex: 1,
                height: 1,
                maxWidth: 60,
              }}
            />
            <span>drag to compare</span>
            <span
              aria-hidden
              style={{
                background: BORDER,
                flex: 1,
                height: 1,
                maxWidth: 60,
              }}
            />
          </div>
        </div>
      </div>
    </section>
  );
}
