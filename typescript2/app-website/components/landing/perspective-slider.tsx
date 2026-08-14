'use client';

import { useReducedMotion } from 'motion/react';
import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { useIsMobile } from '@/hooks/use-media-query';

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

const BAML_CODE = `class LineItem {
    name      string
    quantity  int
    price     float
}

class Invoice {
    vendor      string
    total       float
    due_date    string?
    line_items  LineItem[]
}

class ValidationIssue {
    path      string
    severity  string
    message   string
}

enum RiskTier {
    Low,
    Review,
    Block,
}

class Report {
    risk    RiskTier
    issues  ValidationIssue[]
    total   float
}

function Abs(x: float) -> float {
    if x < 0.0 { -x } else { x }
}

//# add up the line items
function LineTotal(items: LineItem[]) -> float {
    let total = 0.0;
    for (let item in items) {
        total += item.quantity * item.price;
    }
    return total;
}

//# validate the invoice
function ValidateInvoice(inv: Invoice) -> ValidationIssue[] {
    let issues: ValidationIssue[] = [];

    //# check the math
    if Abs(LineTotal(inv.line_items) - inv.total) > 0.02 {
        issues.push(ValidationIssue {
            path: "total",
            severity: "error",
            message: "line item sum does not match total",
        });
    }

    //# check the dates
    if inv.due_date == null {
        issues.push(ValidationIssue {
            path: "due_date",
            severity: "warn",
            message: "missing due date",
        });
    }

    return issues;
}

//# score the risk
function RiskScore(inv: Invoice) -> RiskTier {
    if inv.total > 25000.0 {
        //# block big invoices
        return RiskTier.Block;
    }
    if inv.due_date == null {
        //# flag missing due dates
        return RiskTier.Review;
    }
    return RiskTier.Low;
}

function Review(inv: Invoice) -> Report {
    let risk = RiskScore(inv);
    let issues = ValidateInvoice(inv);
    return Report { risk: risk, issues: issues, total: inv.total };
}

// ── Optional: same pipeline starting from raw text. Requires OPENAI_API_KEY.

client<llm> OpenAI {
    provider openai
    options {
        model "gpt-4o-mini"
        api_key env.OPENAI_API_KEY
    }
}

function ExtractInvoice(text: string) -> Invoice {
    client OpenAI
    prompt #"
        Extract a structured invoice from the text below.

        {{ ctx.output_format }}

        {{ _.role("user") }}
        {{ text }}
    "#
}`;

// ── Code panel (shiki-highlighted) ────────────────────────────────────────────

type CodeToken = { content: string; color?: string };
type CodeTokens = CodeToken[][];

function useTokenizedBaml(code: string): CodeTokens {
  const [out, setOut] = useState<CodeTokens>(() =>
    code.split('\n').map((line) => [{ content: line }] as CodeToken[]),
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
          langs: [bamlJinjaTextmate, bamlTextmate],
          themes: ['github-light'],
        });
        const r = highlighter.codeToTokens(code, {
          // biome-ignore lint/suspicious/noExplicitAny: bamlTextmate lang name
          lang: 'baml' as any,
          theme: 'github-light',
        });
        if (cancelled) return;
        setOut(
          r.tokens.map((line) =>
            line.map((t) => ({ content: t.content, color: t.color })),
          ),
        );
      } catch {
        /* fall back to plain text */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [code]);

  return out;
}

function CodePanel() {
  const tokens = useTokenizedBaml(BAML_CODE);
  const LINE_HEIGHT = 20;
  const LINE_NUM_WIDTH = 44;
  const PAD_TOP = 12;
  const PAD_LEFT = 16;

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
          display: 'flex',
          flex: 1,
          fontFamily: MONO,
          fontSize: 13,
          lineHeight: `${LINE_HEIGHT}px`,
          minHeight: 0,
          overflow: 'auto',
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
            padding: `${PAD_TOP}px 8px`,
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
            padding: `${PAD_TOP}px ${PAD_LEFT}px`,
            tabSize: 4,
            whiteSpace: 'pre',
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

// Single horizontal flow: Invoice → Review() → Report. Pure BAML — no LLM.
const VIEW_W = 760;
const VIEW_H = 360;

const GRAPH_NODES: GraphNode[] = [
  {
    id: 'input',
    kind: 'class',
    label: 'Invoice',
    state: 'success',
    x: 30,
    y: 70,
    w: 220,
    h: 220,
  },
  {
    id: 'review',
    kind: 'function',
    label: 'Review',
    state: 'success',
    x: 296,
    y: 158,
    w: 168,
    h: 44,
  },
  {
    id: 'report',
    kind: 'class',
    label: 'Report',
    state: 'success',
    x: 510,
    y: 70,
    w: 220,
    h: 220,
  },
];

const GRAPH_EDGES: GraphEdge[] = [
  { from: 'input', to: 'review', active: true },
  { from: 'review', to: 'report', active: true },
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

// github-light tokens for the JSON-shaped previews.
const COL = {
  key: '#8250DF',
  str: '#0A3069',
  num: '#0550AE',
  punct: '#6E7781',
  brace: '#1F2328',
  enum: '#953800',
};

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

type FieldRow = { k: string; v: string; vc: string };

function FieldsPreview({ label, rows }: { label: string; rows: FieldRow[] }) {
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
        {label}
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
            }}
          >
            <span style={{ color: COL.key }}>{r.k}</span>
            <span
              style={{
                color: r.vc,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}
            >
              {r.v}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

const INVOICE_ROWS: FieldRow[] = [
  { k: 'vendor', v: '"Acme Corp"', vc: COL.str },
  { k: 'total', v: '1247.50', vc: COL.num },
  { k: 'due_date', v: 'null', vc: COL.enum },
  { k: 'line_items', v: 'LineItem[3]', vc: COL.brace },
];

const REPORT_ROWS: FieldRow[] = [
  { k: 'risk', v: 'RiskTier.Review', vc: COL.enum },
  { k: 'issues', v: 'ValidationIssue[1]', vc: COL.brace },
  { k: 'total', v: '1247.50', vc: COL.num },
];

function GraphPanel({ runningPulse }: { runningPulse: boolean }) {
  const nodeById: Record<string, GraphNode> = Object.fromEntries(
    GRAPH_NODES.map((n) => [n.id, n]),
  );
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [panning, setPanning] = useState(false);
  const panOriginRef = useRef<{
    pointerX: number;
    pointerY: number;
    startX: number;
    startY: number;
  } | null>(null);

  const onPanPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
    panOriginRef.current = {
      pointerX: e.clientX,
      pointerY: e.clientY,
      startX: pan.x,
      startY: pan.y,
    };
    setPanning(true);
  };

  const onPanPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const origin = panOriginRef.current;
    if (!origin) return;
    setPan({
      x: origin.startX + (e.clientX - origin.pointerX),
      y: origin.startY + (e.clientY - origin.pointerY),
    });
  };

  const endPan = (e: ReactPointerEvent<HTMLDivElement>) => {
    panOriginRef.current = null;
    setPanning(false);
    (e.currentTarget as Element).releasePointerCapture?.(e.pointerId);
  };

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
        onPointerCancel={endPan}
        onPointerDown={onPanPointerDown}
        onPointerMove={onPanPointerMove}
        onPointerUp={endPan}
        style={{
          backgroundImage: `radial-gradient(${G.grid} 1px, transparent 1px)`,
          backgroundPosition: `${pan.x}px ${pan.y}px`,
          backgroundSize: '22px 22px',
          cursor: panning ? 'grabbing' : 'grab',
          flex: 1,
          minHeight: 0,
          overflow: 'hidden',
          position: 'relative',
          touchAction: 'none',
          width: '100%',
        }}
      >
        <div
          style={{
            display: 'flex',
            height: '100%',
            justifyContent: 'center',
            padding: '20px 24px',
            transform: `translate(${pan.x}px, ${pan.y}px)`,
            transition: panning ? 'none' : 'transform 120ms ease',
            width: '100%',
            willChange: 'transform',
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

                    if (n.kind === 'class') {
                      const isInput = n.id === 'input';
                      return (
                        <NodeShell key={n.id} n={n}>
                          <NodeHeader accent={accent} n={n} />
                          <FieldsPreview
                            label={
                              isInput ? 'Invoice · input' : 'Report · output'
                            }
                            rows={isInput ? INVOICE_ROWS : REPORT_ROWS}
                          />
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
    const clamped = Math.max(0, Math.min(100, next));
    if (clamped < 18) {
      setPos(0);
      return;
    }
    if (clamped > 82) {
      setPos(100);
      return;
    }
    setPos(clamped);
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
          fontSize: 13,
          fontWeight: 600,
          justifyContent: 'space-between',
          letterSpacing: '0.12em',
          marginBottom: 14,
          padding: '0 4px',
          textTransform: 'uppercase',
        }}
      >
        <span style={{ alignItems: 'center', display: 'flex', gap: 10 }}>
          <span
            aria-hidden
            style={{
              background: ACCENT,
              borderRadius: '50%',
              height: 8,
              width: 8,
            }}
          />
          what your agent sees
        </span>
        <span style={{ alignItems: 'center', display: 'flex', gap: 10 }}>
          what you see
          <span
            aria-hidden
            style={{
              background: INK,
              borderRadius: '50%',
              height: 8,
              width: 8,
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
  const isMobile = useIsMobile();
  const [view, setView] = useState<'agent' | 'you'>('agent');

  const updateFromClientX = useCallback((clientX: number) => {
    const el = containerRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const next = ((clientX - rect.left) / rect.width) * 100;
    const clamped = Math.max(0, Math.min(100, next));
    if (clamped < 18) {
      setPos(0);
      return;
    }
    if (clamped > 82) {
      setPos(100);
      return;
    }
    setPos(clamped);
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

  if (isMobile) {
    const tabStyle = (active: boolean): CSSProperties => ({
      alignItems: 'center',
      background: active ? BG : 'transparent',
      border: 'none',
      borderRadius: 8,
      boxShadow: active ? '0 1px 3px rgba(26,22,18,0.12)' : 'none',
      color: active ? INK : EYEBROW,
      cursor: 'pointer',
      display: 'flex',
      flex: 1,
      fontFamily: MONO,
      fontSize: 11,
      fontWeight: 600,
      gap: 7,
      justifyContent: 'center',
      letterSpacing: '0.08em',
      padding: '10px 6px',
      textTransform: 'uppercase',
      transition: 'background-color 160ms ease, color 160ms ease',
    });

    return (
      <section
        aria-label="What you see vs what your agent sees"
        style={{
          background: BG,
          borderTop: `1px solid ${BORDER}`,
          color: INK,
          padding: '72px 0 88px',
          width: '100%',
        }}
      >
        <style>{`
          @keyframes baml-perspective-dash { to { stroke-dashoffset: -20; } }
          @keyframes baml-perspective-spin { to { transform: rotate(360deg); } }
          @keyframes baml-perspective-blink { 0%, 60% { opacity: 1; } 61%, 100% { opacity: 0; } }
        `}</style>
        <div style={{ margin: '0 auto', maxWidth: 1200, padding: '0 20px' }}>
          <div style={{ marginBottom: 32, textAlign: 'center' }}>
            <h2
              style={{
                color: INK,
                fontSize: 'clamp(1.9rem, 8vw, 2.4rem)',
                fontWeight: 600,
                letterSpacing: '-0.025em',
                lineHeight: 1.08,
                margin: 0,
              }}
            >
              What you see vs.{' '}
              <span style={{ color: ACCENT }}>what your agent sees</span>.
            </h2>
            <p
              style={{
                color: MUTED,
                fontSize: 15,
                lineHeight: 1.6,
                margin: '16px auto 0',
                maxWidth: 540,
              }}
            >
              BAML is the contract both sides read. Your agent gets typed source
              it can navigate, refactor, and test. You get the same program as
              an executable graph. Tap to switch perspectives.
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
            {/* Segmented toggle */}
            <div
              style={{
                background: '#F0ECE0',
                borderRadius: 10,
                display: 'flex',
                gap: 4,
                marginBottom: 12,
                padding: 4,
              }}
            >
              <button
                aria-pressed={view === 'agent'}
                onClick={() => setView('agent')}
                style={tabStyle(view === 'agent')}
                type="button"
              >
                <span
                  aria-hidden
                  style={{
                    background: ACCENT,
                    borderRadius: '50%',
                    height: 8,
                    width: 8,
                  }}
                />
                agent sees
              </button>
              <button
                aria-pressed={view === 'you'}
                onClick={() => setView('you')}
                style={tabStyle(view === 'you')}
                type="button"
              >
                <span
                  aria-hidden
                  style={{
                    background: INK,
                    borderRadius: '50%',
                    height: 8,
                    width: 8,
                  }}
                />
                you see
              </button>
            </div>

            <div
              style={{
                borderRadius: 10,
                height: 'min(520px, 64vh)',
                minHeight: 340,
                overflow: 'hidden',
                position: 'relative',
                width: '100%',
              }}
            >
              {view === 'agent' ? (
                <CodePanel />
              ) : (
                <GraphPanel runningPulse={!reduced} />
              )}
            </div>
          </div>
        </div>
      </section>
    );
  }

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
            What you see vs.{' '}
            <span style={{ color: ACCENT }}>what your agent sees</span>.
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
              fontSize: 14,
              fontWeight: 600,
              justifyContent: 'space-between',
              letterSpacing: '0.14em',
              marginBottom: 14,
              padding: '0 6px',
              textTransform: 'uppercase',
            }}
          >
            <span style={{ alignItems: 'center', display: 'flex', gap: 10 }}>
              <span
                aria-hidden
                style={{
                  background: ACCENT,
                  borderRadius: '50%',
                  height: 8,
                  width: 8,
                }}
              />
              what your agent sees
            </span>
            <span style={{ alignItems: 'center', display: 'flex', gap: 10 }}>
              what you see
              <span
                aria-hidden
                style={{
                  background: INK,
                  borderRadius: '50%',
                  height: 8,
                  width: 8,
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
