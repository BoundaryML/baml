/** biome-ignore-all lint/style/useFilenamingConvention: public component name mirrors ExecutionPanel */
/**
 * Telemetry — the local observability surface for BAML executions.
 *
 * ## Vocabulary (keep these words exact; they mirror CANONICAL/design)
 *
 * - **Execution**: one parentless root thread — a row in the executions
 *   table. It has an *entry point* (the literal command or host that started
 *   it: `baml run …`, `baml test -i …`, playground, an SDK process).
 * - **Span**: one individually retained call with exact timestamps — from
 *   the recent exact window, capture policy (roots, LLM functions, `$id`),
 *   or error promotion. A span is evidence.
 * - **Context**: one calling-context (CCT) aggregate — counts, total/self/
 *   await time, duration histogram for every call that ever took this path.
 *   A context is a summary, complete by construction, with no per-instance
 *   ordering or timestamps.
 * - **Exemplar**: a span linked to its context — the retained instances of
 *   an aggregate path (the metrics-world "exemplar" pattern).
 * - **Gap**: work the aggregates know happened but no span shows. Rendered
 *   as an explicit chip, never as an invented row.
 *
 * ## The two views
 *
 * - **Trace** is ordered by time. It contains only spans; aggregate-only
 *   work appears as gap chips that pivot into Profile.
 * - **Profile** is ordered by call path. It renders the CCT (flame graph +
 *   context tree); retained instances appear as exemplar badges that pivot
 *   into Trace.
 *
 * ## Honesty rules (enforced in pixels)
 *
 * 1. An aggregate never gets a position on a time axis.
 * 2. A count (`×41`) never expands into instance rows unless every instance
 *    was retained; otherwise the exemplar badge says "2 of 41 retained".
 * 3. Aggregate-derived pixels are dashed/faint; span-derived pixels are
 *    solid. The reader can always tell evidence from summary.
 * 4. "Not captured by policy" and "not readable over this connection" are
 *    different states with different labels.
 *
 * ## Wire mapping
 *
 * Live data comes from `/api/obs` (BQF1 frames): `runs` for the table;
 * `run_meta` (dictionary), `left_heavy` (CCT), `timeline` (thread lanes) and
 * `recent_calls` (exact spans, this-process engines only) for the detail.
 * Value bodies are not readable over this socket yet, and spans are joined
 * to contexts by function identity until the recent-calls frame carries the
 * CCT node id; both approximations are labeled in the UI. The two richest
 * executions in the table are prototype examples and say so.
 */

import {
  Activity,
  AlertCircle,
  ArrowLeft,
  ArrowRight,
  BarChart3,
  Braces,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Circle,
  Clock3,
  Database,
  Flame,
  FlaskConical,
  GitBranch,
  Info,
  Layers3,
  Loader2,
  Pause,
  Play,
  Search,
  Settings2,
  Sigma,
  Sparkles,
  Terminal,
  Waypoints,
  XCircle,
  Zap,
} from 'lucide-react';
import type { FC, ReactNode } from 'react';
import { useCallback, useEffect, useMemo, useState } from 'react';

import { cn } from '../lib/utils';
import {
  asLeftHeavy,
  asRecentCalls,
  asRunMeta,
  asRunsList,
  asTimeline,
  FOLD_ROW_FUNCTION,
  FrameKind,
} from './bqf1';
import { defaultObsUrl, WsObserveClient } from './observe-client';

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

type Status = 'succeeded' | 'failed' | 'running' | 'cancelled';
type SourceKind = 'cli' | 'playground' | 'test' | 'sdk';
type CallKind = 'baml' | 'llm' | 'host' | 'spawn';
type ViewMode = 'overview' | 'trace' | 'profile';

interface ExecutionRow {
  id: string;
  /** Root/target function (or test selection) this execution ran. */
  target: string;
  /** The literal command or host that started it — shown verbatim. */
  entryPoint: string;
  sourceKind: SourceKind;
  status: Status;
  startedMs: number;
  /** null while running / unknown. */
  durationMs: number | null;
  /** Total function calls (aggregate enters). null = not reported yet. */
  calls: number | null;
  errors: number | null;
  /** Individually retained spans. null = not reported yet. */
  spanCount: number | null;
  /** Bytes the dedup/CAS layer compressed away. null = not reported. */
  savedBytes: number | null;
  revision: string;
  live?: boolean;
  prototype?: boolean;
  evidence?: Evidence;
}

/** One CCT aggregate node. Complete counts; no per-instance time. */
interface ContextNode {
  id: string;
  parentId: string | null;
  fn: string;
  kind: CallKind;
  enters: number;
  errors: number;
  totalMs: number;
  selfMs: number;
  awaitMs: number | null;
  /** 16 log-scale duration buckets when the reader ships them. */
  histogram?: number[];
  /** Reached through a spawn edge (runs on its own logical thread). */
  spawn?: boolean;
  /** Synthetic "N smaller contexts" fold row from the LOD reader. */
  folded?: number;
  llm?: { model: string; tokensIn: number; tokensOut: number };
}

/** One retained call: exact evidence. */
interface SpanNode {
  id: string;
  /** Nearest *retained* ancestor (structural parent may be summary-only). */
  parentId: string | null;
  /** CCT join. Approximated by function identity for live data (labeled). */
  contextId: string | null;
  fn: string;
  kind: CallKind;
  threadName: string;
  startMs: number;
  durationMs: number;
  selfMs?: number;
  status: Status;
  /** Why this call was individually retained. */
  reason: string;
  args?: unknown;
  result?: unknown;
  error?: unknown;
  cvalues?: Array<{ name: string; type: string; value: unknown }>;
  inputBytes?: number;
  outputBytes?: number;
  /** Why value bodies are absent (distinct honesty states, rule 4). */
  valuesUnavailable?: 'policy' | 'connection';
}

/** Aggregate-only work between spans: rendered as a chip, never a bar. */
interface GapInfo {
  id: string;
  /** Retained span whose subtree this summarizes; null = whole execution. */
  parentSpanId: string | null;
  calls: number;
  functions: string[];
  /** Where the "view aggregate" pivot lands in Profile. */
  contextId: string;
  /** policy = never selected for retention; window = aged out of the ring. */
  reason: 'policy' | 'window';
}

interface ThreadLane {
  id: string;
  name: string;
  firstMs: number;
  lastMs: number;
  busyMs: number | null;
  awaitMs: number | null;
  errors: number | null;
}

interface Evidence {
  contexts: ContextNode[];
  spans: SpanNode[];
  gaps: GapInfo[];
  threads: ThreadLane[];
  /** Value bodies readable here? false → "connection" unavailability. */
  valuesWired: boolean;
  /** Span→context join fidelity (live data joins by function id for now). */
  exactContextJoin: boolean;
}

const KB = 1024;
const MB = 1024 * KB;

// ---------------------------------------------------------------------------
// Prototype examples — clearly labeled in the UI. Two density regimes:
// a dense checkout run (most calls retained via the recent window) and a
// long research-agent run in a separate SDK process (no window access from
// this server), where retention is exactly the default capture policy: the
// root and every LLM call carry spans with bodies; non-LLM helpers exist
// only as aggregates. LLM spans are generated from the aggregate counts so
// the example cannot drift from the policy, and table-row totals derive
// from the evidence for the same reason.
// ---------------------------------------------------------------------------

function prototypeStartedMs(time: string): number {
  const [hours, minutes, seconds] = time.split(':').map(Number);
  const started = new Date();
  started.setHours(hours ?? 0, minutes ?? 0, seconds ?? 0, 0);
  if (started.getTime() > Date.now()) started.setDate(started.getDate() - 1);
  return started.getTime();
}

const CHECKOUT_EVIDENCE: Evidence = {
  contexts: [
    {
      awaitMs: 1240,
      enters: 1,
      errors: 1,
      fn: 'ProcessCheckout',
      id: 'c-root',
      kind: 'baml',
      parentId: null,
      selfMs: 34,
      totalMs: 1840,
    },
    {
      awaitMs: 0,
      enters: 1,
      errors: 0,
      fn: 'LoadCart',
      id: 'c-load',
      kind: 'host',
      parentId: 'c-root',
      selfMs: 74,
      totalMs: 74,
    },
    {
      awaitMs: 0,
      enters: 3,
      errors: 0,
      fn: 'ValidateInventory',
      id: 'c-validate',
      kind: 'baml',
      parentId: 'c-root',
      selfMs: 96,
      totalMs: 96,
    },
    {
      awaitMs: 0,
      enters: 41,
      errors: 0,
      fn: 'CalculatePrice',
      histogram: [0, 2, 6, 11, 9, 6, 4, 2, 1, 0, 0, 0, 0, 0, 0, 0],
      id: 'c-price',
      kind: 'baml',
      parentId: 'c-root',
      selfMs: 512,
      totalMs: 512,
    },
    {
      awaitMs: 5,
      enters: 1,
      errors: 0,
      fn: 'AssessFraudRisk',
      id: 'c-fraud',
      kind: 'baml',
      parentId: 'c-root',
      selfMs: 23,
      totalMs: 728,
    },
    {
      awaitMs: 670,
      enters: 1,
      errors: 0,
      fn: 'FraudSignals',
      id: 'c-fraudllm',
      kind: 'llm',
      llm: { model: 'gpt-5-mini', tokensIn: 1248, tokensOut: 86 },
      parentId: 'c-fraud',
      selfMs: 12,
      totalMs: 682,
    },
    {
      awaitMs: 48,
      enters: 1,
      errors: 1,
      fn: 'ChargePayment',
      id: 'c-charge',
      kind: 'baml',
      parentId: 'c-root',
      selfMs: 51,
      totalMs: 804,
    },
    {
      awaitMs: 0,
      enters: 2,
      errors: 1,
      fn: 'RetryProviderCall',
      id: 'c-retry',
      kind: 'host',
      parentId: 'c-charge',
      selfMs: 705,
      totalMs: 705,
    },
    {
      awaitMs: 1620,
      enters: 1,
      errors: 0,
      fn: 'WriteAuditLog',
      id: 'c-audit',
      kind: 'spawn',
      parentId: 'c-root',
      selfMs: 40,
      spawn: true,
      totalMs: 1660,
    },
  ],
  exactContextJoin: true,
  gaps: [
    {
      calls: 43,
      contextId: 'c-root',
      functions: ['CalculatePrice', 'ValidateInventory', 'LoadCart'],
      id: 'g-root',
      parentSpanId: 's-root',
      reason: 'policy',
    },
    {
      calls: 2,
      contextId: 'c-retry',
      functions: ['RetryProviderCall'],
      id: 'g-charge',
      parentSpanId: 's-charge',
      reason: 'policy',
    },
  ],
  spans: [
    {
      args: {
        cart_id: 'cart_7YC4',
        customer: { id: 'cus_1284', tier: 'gold' },
        items: 3,
      },
      contextId: 'c-root',
      durationMs: 1840,
      error: {
        code: 'card_declined',
        message: 'Card was declined after fraud review',
        type: 'PaymentDeclined',
      },
      fn: 'ProcessCheckout',
      id: 's-root',
      inputBytes: 1840,
      kind: 'baml',
      outputBytes: 612,
      parentId: null,
      reason: 'root call — always captured',
      selfMs: 34,
      startMs: 0,
      status: 'failed',
      threadName: 'main',
    },
    {
      args: { cart_total: 428.5, customer_age_days: 91, prior_orders: 2 },
      contextId: 'c-fraud',
      cvalues: [
        { name: 'risk_score', type: 'float', value: 0.78 },
        { name: 'decision', type: 'enum', value: 'REVIEW' },
      ],
      durationMs: 728,
      fn: 'AssessFraudRisk',
      id: 's-fraud',
      inputBytes: 268,
      kind: 'baml',
      outputBytes: 124,
      parentId: 's-root',
      reason: 'explicit capture ($id)',
      result: {
        reasons: ['velocity', 'new_device'],
        risk: 'review',
        score: 0.78,
      },
      selfMs: 23,
      startMs: 110,
      status: 'succeeded',
      threadName: 'main',
    },
    {
      args: { input_tokens: 1248, model: 'gpt-5-mini' },
      contextId: 'c-fraudllm',
      durationMs: 682,
      fn: 'FraudSignals',
      id: 's-fraudllm',
      inputBytes: 6200,
      kind: 'llm',
      outputBytes: 386,
      parentId: 's-fraud',
      reason: 'LLM capture policy',
      result: { category: 'review', score: 0.78 },
      selfMs: 12,
      startMs: 136,
      status: 'succeeded',
      threadName: 'main',
    },
    {
      args: { cart_id: 'cart_7YC4', event: 'checkout_started' },
      contextId: 'c-audit',
      durationMs: 1660,
      fn: 'WriteAuditLog',
      id: 's-audit',
      inputBytes: 91,
      kind: 'spawn',
      outputBytes: 0,
      parentId: 's-root',
      reason: 'spawned thread root',
      selfMs: 40,
      startMs: 142,
      status: 'cancelled',
      threadName: 'audit-1',
    },
    {
      contextId: 'c-price',
      durationMs: 14,
      fn: 'CalculatePrice',
      id: 's-price17',
      kind: 'baml',
      parentId: 's-root',
      reason: 'recent exact window',
      startMs: 855,
      status: 'succeeded',
      threadName: 'main',
      valuesUnavailable: 'policy',
    },
    {
      contextId: 'c-price',
      durationMs: 11,
      fn: 'CalculatePrice',
      id: 's-price38',
      kind: 'baml',
      parentId: 's-root',
      reason: 'recent exact window',
      startMs: 1421,
      status: 'succeeded',
      threadName: 'main',
      valuesUnavailable: 'policy',
    },
    {
      args: { amount: 428.5, currency: 'USD', payment_method: 'pm_•••• 4242' },
      contextId: 'c-charge',
      cvalues: [
        { name: 'attempt', type: 'int', value: 2 },
        { name: 'payment_provider', type: 'string', value: 'stripe' },
      ],
      durationMs: 804,
      error: {
        message: 'Your card was declined.',
        provider: 'stripe',
        status: 402,
        type: 'ProviderError',
      },
      fn: 'ChargePayment',
      id: 's-charge',
      inputBytes: 312,
      kind: 'baml',
      outputBytes: 288,
      parentId: 's-root',
      reason: 'errored call — promoted',
      selfMs: 51,
      startMs: 1002,
      status: 'failed',
      threadName: 'main',
    },
  ],
  threads: [
    {
      awaitMs: 1320,
      busyMs: 520,
      errors: 2,
      firstMs: 0,
      id: 'main',
      lastMs: 1840,
      name: 'main',
    },
    {
      awaitMs: 1620,
      busyMs: 40,
      errors: 0,
      firstMs: 142,
      id: 'audit-1',
      lastMs: 1802,
      name: 'audit-1 · spawned by ProcessCheckout',
    },
  ],
  valuesWired: true,
};

const RESEARCH_TOTAL_MS = 252_000;

/** Deterministic 32-bit mix — stable pseudo-jitter without Math.random. */
function hash32(seed: number): number {
  let x = seed | 0;
  x = Math.imul(x ^ (x >>> 16), 0x45d9f3b);
  x = Math.imul(x ^ (x >>> 16), 0x45d9f3b);
  return (x ^ (x >>> 16)) >>> 0;
}

/**
 * Generate the spans the default capture policy implies: every call of an
 * LLM function is retained with bodies. Counts, error counts, and the time
 * range come from the aggregate, so the spans cannot drift from the CCT.
 */
function generatePolicyLlmSpans(opts: {
  contextId: string;
  parentSpanId: string;
  fn: string;
  model: string;
  count: number;
  /** Mark call i failed when `i % errorEvery === errorPhase` (chosen so the
      failure count equals the context's aggregate `errors`). */
  errorEvery: number;
  errorPhase: number;
  firstMs: number;
  lastMs: number;
}): SpanNode[] {
  const spans: SpanNode[] = [];
  const spacing = (opts.lastMs - opts.firstMs) / opts.count;
  for (let i = 0; i < opts.count; i += 1) {
    const h = hash32(i ^ Math.imul(opts.contextId.length, 2654435761));
    const startMs =
      opts.firstMs + i * spacing + (h % Math.max(1, Math.floor(spacing * 0.5)));
    const durationMs = 150 + ((h >>> 8) % 700);
    const failed = i % opts.errorEvery === opts.errorPhase;
    spans.push({
      args: { model: opts.model, seq: i },
      contextId: opts.contextId,
      durationMs,
      error: failed
        ? { retry_after_ms: 800, status: 529, type: 'ProviderOverloaded' }
        : undefined,
      fn: opts.fn,
      id: `${opts.contextId}-i${i}`,
      inputBytes: 380 + (h % 300),
      kind: 'llm',
      outputBytes: failed ? 0 : 60 + (h % 40),
      parentId: opts.parentSpanId,
      reason: 'LLM capture policy',
      result: failed
        ? undefined
        : { relevant: h % 3 !== 0, score: (h % 100) / 100 },
      startMs,
      status: failed ? 'failed' : 'succeeded',
      threadName: 'main',
    });
  }
  return spans;
}

const RESEARCH_LLM_SPANS: SpanNode[] = [
  // ScoreRelevance: ×402 with 12 errors (i % 33 === 7 fires 12 times in 402).
  ...generatePolicyLlmSpans({
    contextId: 'r-score',
    count: 402,
    errorEvery: 33,
    errorPhase: 7,
    firstMs: 6_000,
    fn: 'ScoreRelevance',
    lastMs: 249_000,
    model: 'claude-haiku-4-5-20251001',
    parentSpanId: 's-r-root',
  }),
  // SummarizeChunk: ×128 with 4 errors (i % 32 === 5 fires 4 times in 128).
  ...generatePolicyLlmSpans({
    contextId: 'r-chunk',
    count: 128,
    errorEvery: 32,
    errorPhase: 5,
    firstMs: 9_500,
    fn: 'SummarizeChunk',
    lastMs: 243_000,
    model: 'claude-haiku-4-5-20251001',
    parentSpanId: 's-r-root',
  }),
];

const RESEARCH_EVIDENCE: Evidence = {
  contexts: [
    {
      awaitMs: 214_000,
      enters: 1,
      errors: 0,
      fn: 'ResearchAgent',
      id: 'r-root',
      kind: 'baml',
      parentId: null,
      selfMs: 1_800,
      totalMs: RESEARCH_TOTAL_MS,
    },
    {
      awaitMs: 188_000,
      enters: 24,
      errors: 0,
      fn: 'PlanStep',
      id: 'r-plan',
      kind: 'baml',
      parentId: 'r-root',
      selfMs: 9_000,
      totalMs: 236_000,
    },
    {
      awaitMs: 0,
      enters: 96,
      errors: 9,
      fn: 'SearchWeb',
      id: 'r-search',
      kind: 'host',
      parentId: 'r-plan',
      selfMs: 64_000,
      totalMs: 64_000,
    },
    {
      awaitMs: 0,
      enters: 310,
      errors: 12,
      fn: 'ReadDocument',
      id: 'r-read',
      kind: 'host',
      parentId: 'r-plan',
      selfMs: 58_000,
      totalMs: 58_000,
    },
    {
      awaitMs: 94_500,
      enters: 128,
      errors: 4,
      fn: 'SummarizeChunk',
      histogram: [0, 0, 1, 3, 9, 21, 34, 30, 18, 8, 3, 1, 0, 0, 0, 0],
      id: 'r-chunk',
      kind: 'llm',
      llm: {
        model: 'claude-haiku-4-5-20251001',
        tokensIn: 412_000,
        tokensOut: 38_500,
      },
      parentId: 'r-plan',
      selfMs: 1_500,
      totalMs: 96_000,
    },
    {
      awaitMs: 139_000,
      enters: 402,
      errors: 12,
      fn: 'ScoreRelevance',
      histogram: [0, 4, 18, 61, 118, 97, 58, 27, 12, 5, 2, 0, 0, 0, 0, 0],
      id: 'r-score',
      kind: 'llm',
      llm: {
        model: 'claude-haiku-4-5-20251001',
        tokensIn: 188_000,
        tokensOut: 9_100,
      },
      parentId: 'r-plan',
      selfMs: 3_800,
      totalMs: 143_000,
    },
    {
      awaitMs: 8_100,
      enters: 1,
      errors: 0,
      fn: 'WriteReport',
      id: 'r-report',
      kind: 'llm',
      llm: { model: 'claude-sonnet-5', tokensIn: 51_000, tokensOut: 6_400 },
      parentId: 'r-root',
      selfMs: 300,
      totalMs: 8_400,
    },
  ],
  exactContextJoin: true,
  // Non-LLM helpers are the only summary-only work under default policy;
  // this run is another process, so the recent window is not reachable and
  // adds nothing on top of policy retention.
  gaps: [
    {
      calls: 24 + 96 + 310,
      contextId: 'r-root',
      functions: ['ReadDocument', 'SearchWeb', 'PlanStep'],
      id: 'g-research',
      parentSpanId: 's-r-root',
      reason: 'policy',
    },
  ],
  spans: [
    {
      args: { max_steps: 24, question: 'Compare vector DB pricing models' },
      contextId: 'r-root',
      durationMs: RESEARCH_TOTAL_MS,
      fn: 'ResearchAgent',
      id: 's-r-root',
      inputBytes: 240,
      kind: 'baml',
      outputBytes: 18_400,
      parentId: null,
      reason: 'root call — always captured',
      result: { citations: 41, report: '§ elided — 18.4 KB' },
      selfMs: 1_800,
      startMs: 0,
      status: 'succeeded',
      threadName: 'main',
    },
    {
      args: { citations: 41, style: 'brief' },
      contextId: 'r-report',
      durationMs: 8_400,
      fn: 'WriteReport',
      id: 's-r-report',
      kind: 'llm',
      parentId: 's-r-root',
      reason: 'LLM capture policy',
      result: { tokens_out: 6_400 },
      startMs: 243_400,
      status: 'succeeded',
      threadName: 'main',
    },
    ...RESEARCH_LLM_SPANS,
  ],
  threads: [
    {
      awaitMs: 214_000,
      busyMs: 38_000,
      errors: 37,
      firstMs: 0,
      id: 'main',
      lastMs: RESEARCH_TOTAL_MS,
      name: 'main',
    },
  ],
  valuesWired: true,
};

const TESTRUN_EVIDENCE: Evidence = {
  contexts: [
    {
      awaitMs: null,
      enters: 1,
      errors: 1,
      fn: 'baml test',
      id: 't-root',
      kind: 'host',
      parentId: null,
      selfMs: 400,
      totalMs: 8_400,
    },
    {
      awaitMs: null,
      enters: 6,
      errors: 1,
      fn: 'ProcessCheckout',
      id: 't-proc',
      kind: 'baml',
      parentId: 't-root',
      selfMs: 300,
      totalMs: 7_600,
    },
    {
      awaitMs: null,
      enters: 6,
      errors: 0,
      fn: 'AssessFraudRisk',
      id: 't-fraud',
      kind: 'baml',
      parentId: 't-proc',
      selfMs: 140,
      totalMs: 4_400,
    },
    {
      awaitMs: null,
      enters: 6,
      errors: 0,
      fn: 'FraudSignals',
      id: 't-fraudllm',
      kind: 'llm',
      llm: { model: 'gpt-5-mini', tokensIn: 7_400, tokensOut: 512 },
      parentId: 't-fraud',
      selfMs: 70,
      totalMs: 4_100,
    },
  ],
  exactContextJoin: true,
  gaps: [
    {
      calls: 6 + 6,
      contextId: 't-root',
      functions: ['ProcessCheckout', 'AssessFraudRisk'],
      id: 'g-test',
      parentSpanId: 's-t-root',
      reason: 'policy',
    },
  ],
  spans: [
    // FraudSignals is LLM ×6 → default policy retains all six with bodies
    // (errorEvery 7 / phase 6 never fires within 6 calls: 0 errors).
    ...generatePolicyLlmSpans({
      contextId: 't-fraudllm',
      count: 6,
      errorEvery: 7,
      errorPhase: 6,
      firstMs: 700,
      fn: 'FraudSignals',
      lastMs: 7_900,
      model: 'gpt-5-mini',
      parentSpanId: 's-t-root',
    }),
    {
      args: { filter: 'commerce/**', tests: 6 },
      contextId: 't-root',
      durationMs: 8_400,
      fn: 'baml test',
      id: 's-t-root',
      kind: 'host',
      parentId: null,
      reason: 'root call — always captured',
      result: { failed: 1, passed: 5 },
      startMs: 0,
      status: 'failed',
      threadName: 'main',
    },
  ],
  threads: [
    {
      awaitMs: null,
      busyMs: null,
      errors: 1,
      firstMs: 0,
      id: 'main',
      lastMs: 8_400,
      name: 'main',
    },
  ],
  valuesWired: true,
};

/** Table totals derive from the aggregates so rows cannot drift from them. */
function evidenceTotals(evidence: Evidence): { calls: number; errors: number } {
  const contexts = evidence.contexts.filter((context) => !context.folded);
  return {
    calls: contexts.reduce((sum, context) => sum + context.enters, 0),
    errors: contexts.reduce((sum, context) => sum + context.errors, 0),
  };
}

const PROTOTYPE_EXECUTIONS: ExecutionRow[] = [
  {
    ...evidenceTotals(CHECKOUT_EVIDENCE),
    durationMs: 1840,
    // Playground-launched: same process as this server, so recent-window
    // spans are genuinely reachable (they are not for other-process runs).
    entryPoint: 'playground · run ProcessCheckout',
    evidence: CHECKOUT_EVIDENCE,
    id: 'proto-checkout',
    prototype: true,
    revision: 'cct-1@32b9fe5',
    savedBytes: 42.7 * KB,
    sourceKind: 'playground',
    spanCount: CHECKOUT_EVIDENCE.spans.length,
    startedMs: prototypeStartedMs('11:42:01'),
    status: 'failed',
    target: 'ProcessCheckout',
  },
  {
    ...evidenceTotals(RESEARCH_EVIDENCE),
    durationMs: RESEARCH_TOTAL_MS,
    entryPoint: 'python agents/research.py',
    evidence: RESEARCH_EVIDENCE,
    id: 'proto-research',
    prototype: true,
    revision: 'cct-1@32b9fe5',
    savedBytes: 3.1 * MB,
    sourceKind: 'sdk',
    spanCount: RESEARCH_EVIDENCE.spans.length,
    startedMs: prototypeStartedMs('11:27:08'),
    status: 'succeeded',
    target: 'ResearchAgent',
  },
  {
    ...evidenceTotals(TESTRUN_EVIDENCE),
    durationMs: 8_400,
    entryPoint: 'baml test -i "commerce/**"',
    evidence: TESTRUN_EVIDENCE,
    id: 'proto-test',
    prototype: true,
    revision: 'cct-1@df7e52f',
    savedBytes: 214 * KB,
    sourceKind: 'test',
    spanCount: TESTRUN_EVIDENCE.spans.length,
    startedMs: prototypeStartedMs('10:56:03'),
    status: 'failed',
    target: 'commerce test suite',
  },
];

// ---------------------------------------------------------------------------
// Formatting and shared atoms
// ---------------------------------------------------------------------------

function formatDuration(ms: number): string {
  if (ms >= 60_000) {
    const minutes = Math.floor(ms / 60_000);
    const seconds = Math.floor((ms % 60_000) / 1000);
    return `${minutes}m ${seconds}s`;
  }
  if (ms >= 1000) return `${(ms / 1000).toFixed(ms >= 10_000 ? 1 : 2)}s`;
  if (ms >= 1) return `${Math.round(ms)}ms`;
  return `${ms.toFixed(2)}ms`;
}

function formatBytes(bytes: number): string {
  if (bytes >= MB) return `${(bytes / MB).toFixed(1)} MB`;
  if (bytes >= KB) return `${(bytes / KB).toFixed(1)} KB`;
  return `${Math.round(bytes)} B`;
}

function formatCount(value: number | null): string {
  return value === null ? '—' : value.toLocaleString();
}

function normalizeStatus(status: string): Status {
  switch (status) {
    case 'running':
      return 'running';
    case 'failed':
    case 'crashed':
    case 'error':
      return 'failed';
    case 'cancelled':
      return 'cancelled';
    default:
      return 'succeeded';
  }
}

function shortRevision(revision: string): string {
  return revision.length > 13 ? revision.slice(0, 13) : revision;
}

function functionColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1)
    hash = (hash * 31 + name.charCodeAt(i)) | 0;
  return `hsl(${Math.abs(hash) % 360} 56% 52%)`;
}

function statusStyles(status: Status): string {
  if (status === 'failed')
    return 'text-vsc-red bg-vsc-red/10 border-vsc-red/25';
  if (status === 'running')
    return 'text-vsc-accent bg-vsc-accent/10 border-vsc-accent/25';
  if (status === 'cancelled')
    return 'text-vsc-yellow bg-vsc-yellow/10 border-vsc-yellow/25';
  return 'text-vsc-green bg-vsc-green/10 border-vsc-green/25';
}

const StatusIcon: FC<{ status: Status; className?: string }> = ({
  status,
  className,
}) => {
  if (status === 'failed')
    return <XCircle className={cn('h-3.5 w-3.5', className)} />;
  if (status === 'running')
    return <Loader2 className={cn('h-3.5 w-3.5 animate-spin', className)} />;
  if (status === 'cancelled')
    return <Pause className={cn('h-3.5 w-3.5', className)} />;
  return <CheckCircle2 className={cn('h-3.5 w-3.5', className)} />;
};

const SourceIcon: FC<{ sourceKind: SourceKind; className?: string }> = ({
  sourceKind,
  className,
}) => {
  const cls = cn('h-3 w-3', className);
  if (sourceKind === 'test') return <FlaskConical className={cls} />;
  if (sourceKind === 'playground') return <Play className={cls} />;
  if (sourceKind === 'sdk') return <Activity className={cls} />;
  return <Terminal className={cls} />;
};

/** Kind glyph: solid chip = evidence-capable call; sparkles = LLM. */
const KindGlyph: FC<{ fn: string; kind: CallKind }> = ({ fn, kind }) => {
  if (kind === 'llm')
    return <Sparkles className="h-3 w-3 shrink-0 text-vsc-accent" />;
  if (kind === 'spawn')
    return <GitBranch className="h-3 w-3 shrink-0 text-vsc-text-muted" />;
  return (
    <span
      className="h-2 w-2 shrink-0 rounded-sm"
      style={{ backgroundColor: functionColor(fn) }}
    />
  );
};

const Pill: FC<{ children: ReactNode; className?: string }> = ({
  children,
  className,
}) => (
  <span
    className={cn(
      'inline-flex items-center gap-1 rounded-full border border-vsc-border-subtle bg-vsc-surface px-2 py-0.5 text-[10px] text-vsc-text-muted',
      className,
    )}
  >
    {children}
  </span>
);

const SectionHeading: FC<{ children: ReactNode }> = ({ children }) => (
  <div className="mb-1.5 text-[9px] font-semibold uppercase tracking-wide text-vsc-text-muted">
    {children}
  </div>
);

const SmallCard: FC<{ label: string; value: string; tone?: string }> = ({
  label,
  value,
  tone,
}) => (
  <div className="rounded border border-vsc-border-subtle bg-vsc-surface p-2">
    <div className="text-[9px] text-vsc-text-faint">{label}</div>
    <div className={cn('mt-1 font-vsc-mono text-[12px]', tone)}>{value}</div>
  </div>
);

const ValueBlock: FC<{ label?: string; value: unknown; tone?: 'error' }> = ({
  label,
  value,
  tone,
}) => (
  <div>
    {label && <SectionHeading>{label}</SectionHeading>}
    <pre
      className={cn(
        'max-h-60 overflow-auto whitespace-pre-wrap rounded border border-vsc-border-subtle bg-vsc-surface p-2 font-vsc-mono text-[10px] leading-4 text-vsc-text',
        tone === 'error' && 'border-vsc-red/25 bg-vsc-red/5',
      )}
    >
      {JSON.stringify(value, null, 2)}
    </pre>
  </div>
);

const Panel: FC<{
  title: string;
  subtitle?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}> = ({ title, subtitle, action, children, className }) => (
  <section
    className={cn(
      'overflow-hidden rounded-md border border-vsc-border bg-vsc-surface',
      className,
    )}
  >
    <div className="flex items-start border-b border-vsc-border-subtle px-3 py-2">
      <div className="min-w-0 flex-1">
        <h2 className="text-[11px] font-semibold">{title}</h2>
        {subtitle && (
          <p className="mt-0.5 truncate text-[10px] text-vsc-text-faint">
            {subtitle}
          </p>
        )}
      </div>
      {action}
    </div>
    {children}
  </section>
);

/** Tiny duration-histogram sparkline (aggregate-derived → muted tone). */
const HistSpark: FC<{ buckets: number[] }> = ({ buckets }) => {
  const max = Math.max(1, ...buckets);
  return (
    <span
      aria-label="duration distribution"
      className="flex h-3.5 items-end gap-px"
      role="img"
    >
      {buckets.map((count, index) => (
        <span
          className="w-[3px] rounded-t-[1px] bg-vsc-accent/45"
          // biome-ignore lint/suspicious/noArrayIndexKey: buckets are positional
          key={index}
          style={{
            height: `${Math.max(count > 0 ? 12 : 4, (count / max) * 100)}%`,
          }}
        />
      ))}
    </span>
  );
};

// ---------------------------------------------------------------------------
// Live evidence — built from /api/obs frames for non-prototype executions.
// ---------------------------------------------------------------------------

function shortFnName(fqn: string): string {
  const last = fqn.split(/[.:]/).pop();
  return last && last.length > 0 ? last : fqn;
}

interface LiveFrameState {
  fnNames: Map<number, string> | null;
  leftHeavy: ReturnType<typeof asLeftHeavy> | null;
  timeline: ReturnType<typeof asTimeline> | null;
  recent: ReturnType<typeof asRecentCalls> | null;
}

function buildLiveEvidence(state: LiveFrameState): Evidence {
  const fnName = (id: number): string => {
    const name = state.fnNames?.get(id);
    return name ? shortFnName(name) : `fn#${id}`;
  };

  // Contexts: left_heavy rows arrive in DFS order with explicit depth.
  const contexts: ContextNode[] = [];
  const contextByFn = new Map<string, string>();
  if (state.leftHeavy) {
    const stack: Array<{ id: string; depth: number }> = [];
    const lh = state.leftHeavy;
    for (let i = 0; i < lh.depth.length; i += 1) {
      const depth = lh.depth[i] ?? 0;
      while (stack.length > 0 && (stack.at(-1)?.depth ?? 0) >= depth)
        stack.pop();
      const parentId = stack.at(-1)?.id ?? null;
      const id = `lh-${i}`;
      const isFold = lh.functionId[i] === FOLD_ROW_FUNCTION;
      const fn = isFold
        ? `${(lh.foldedCount[i] ?? 0).toLocaleString()} smaller contexts`
        : fnName(lh.functionId[i] ?? 0);
      contexts.push({
        awaitMs: null,
        enters: lh.enters[i] ?? 0,
        errors: lh.errors[i] ?? 0,
        fn,
        folded: isFold ? (lh.foldedCount[i] ?? 0) : undefined,
        id,
        kind: 'baml',
        parentId,
        selfMs: (lh.selfNs[i] ?? 0) / 1e6,
        totalMs: (lh.totalNs[i] ?? 0) / 1e6,
      });
      if (!isFold && !contextByFn.has(fn)) contextByFn.set(fn, id);
      stack.push({ depth, id });
    }
  }

  // Baseline: subtract in BigInt space (epoch-ns exceeds Number precision).
  let baseNs: bigint | null = null;
  const consider = (value: bigint) => {
    if (value > 0n && (baseNs === null || value < baseNs)) baseNs = value;
  };
  if (state.recent) for (const ts of state.recent.startNs) consider(ts);
  const toMs = (ns: bigint): number =>
    baseNs === null ? 0 : Number(ns - baseNs) / 1e6;

  // Spans: recent exact window. Parent linkage stays within retained rows;
  // a parent outside the window degrades to a top-level row (never invented).
  const spans: SpanNode[] = [];
  if (state.recent) {
    const rc = state.recent;
    const keyOf = (thread: bigint, call: bigint) => `${thread}:${call}`;
    const present = new Set<string>();
    for (let i = 0; i < rc.callId.length; i += 1)
      present.add(keyOf(rc.thread[i]!, rc.callId[i]!));
    for (let i = 0; i < rc.callId.length; i += 1) {
      const fn = fnName(rc.functionId[i] ?? 0);
      const statusCode = rc.status[i] ?? 0;
      const parentKey = keyOf(rc.thread[i]!, rc.parentCallId[i]!);
      spans.push({
        // TODO(wire): join by CCT node id once RecentCalls carries it; the
        // function-identity join below can be ambiguous for shared helpers.
        contextId: contextByFn.get(fn) ?? null,
        durationMs: toMs(rc.endNs[i]!) - toMs(rc.startNs[i]!),
        fn,
        id: keyOf(rc.thread[i]!, rc.callId[i]!),
        kind: 'baml',
        parentId: present.has(parentKey) ? parentKey : null,
        reason: 'recent exact window',
        startMs: toMs(rc.startNs[i]!),
        status:
          statusCode === 1
            ? 'failed'
            : statusCode === 2
              ? 'cancelled'
              : 'succeeded',
        threadName: `t${(rc.thread[i]! & 0xffffffffn).toString()}`,
        valuesUnavailable: 'connection',
      });
    }
    spans.sort((a, b) => a.startMs - b.startMs);
  }

  // Threads (timeline lanes). Timestamps here are already Number-converted
  // upstream; they share the engine clock with recent calls.
  const threads: ThreadLane[] = [];
  if (state.timeline) {
    const tl = state.timeline;
    const firstAll = Math.min(
      ...Array.from(tl.firstTsNs),
      Number.POSITIVE_INFINITY,
    );
    for (let i = 0; i < tl.thread.length; i += 1) {
      threads.push({
        awaitMs: (tl.awaitNs[i] ?? 0) / 1e6,
        busyMs: (tl.busyNs[i] ?? 0) / 1e6,
        errors: tl.errors[i] ?? 0,
        firstMs: ((tl.firstTsNs[i] ?? firstAll) - firstAll) / 1e6,
        id: tl.thread[i]!.toString(16),
        lastMs: ((tl.lastTsNs[i] ?? firstAll) - firstAll) / 1e6,
        name: `thread · ${fnName(tl.dominantFunction[i] ?? 0)}`,
      });
    }
  }

  // One honest run-level gap when aggregates report more calls than the
  // window retained. Placement inside the run is unknown — so it gets no
  // position, just a chip at the end of the trace.
  const gaps: GapInfo[] = [];
  const totalEnters = contexts
    .filter((c) => !c.folded)
    .reduce((sum, c) => sum + c.enters, 0);
  if (contexts.length > 0 && totalEnters > spans.length) {
    const retainedFns = new Set(spans.map((s) => s.fn));
    gaps.push({
      calls: totalEnters - spans.length,
      contextId: contexts[0]!.id,
      functions: contexts
        .filter((c) => !c.folded && !retainedFns.has(c.fn))
        .sort((a, b) => b.enters - a.enters)
        .slice(0, 5)
        .map((c) => c.fn),
      id: 'g-live-window',
      parentSpanId: null,
      reason: 'window',
    });
  }

  return {
    contexts,
    exactContextJoin: false,
    gaps,
    spans,
    threads,
    valuesWired: false,
  };
}

/** Query + subscribe the per-run frames and fold them into Evidence. */
function useLiveEvidence(
  client: WsObserveClient,
  row: ExecutionRow | null,
): Evidence | null {
  const [evidence, setEvidence] = useState<Evidence | null>(null);

  useEffect(() => {
    setEvidence(null);
    if (!row?.live) return;
    const run = row.id;
    const state: LiveFrameState = {
      fnNames: null,
      leftHeavy: null,
      recent: null,
      timeline: null,
    };
    let disposed = false;
    const rebuild = () => {
      if (!disposed) setEvidence(buildLiveEvidence(state));
    };
    // run_meta is query-only (dictionary is immutable per revision).
    client
      .query('run_meta', { run })
      .then((frame) => {
        const meta = asRunMeta(frame);
        const map = new Map<number, string>();
        for (let i = 0; i < meta.functionId.length; i += 1)
          map.set(meta.functionId[i] ?? 0, meta.fqn[i] ?? '');
        state.fnNames = map;
        rebuild();
      })
      .catch(() => undefined);
    const offs = [
      client.subscribe('left_heavy', { pixelWidth: 1024, run }, (frame) => {
        if (frame.kind !== FrameKind.LeftHeavy) return;
        state.leftHeavy = asLeftHeavy(frame);
        rebuild();
      }),
      client.subscribe('timeline', { run }, (frame) => {
        if (frame.kind !== FrameKind.Timeline) return;
        state.timeline = asTimeline(frame);
        rebuild();
      }),
      client.subscribe('recent_calls', { run }, (frame) => {
        if (frame.kind !== FrameKind.RecentCalls) return;
        state.recent = asRecentCalls(frame);
        rebuild();
      }),
    ];
    return () => {
      disposed = true;
      for (const off of offs) off();
    };
  }, [client, row?.id, row?.live]);

  return evidence;
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

export interface ObsTelemetryTabProps {
  /** Override the `/api/obs` URL (default: mirrors the `/api/ws` derivation). */
  obsUrl?: string;
}

/** Owns the local observability connection; no telemetry leaves the machine. */
export const ObsTelemetryTab: FC<ObsTelemetryTabProps> = ({ obsUrl }) => {
  const [client, setClient] = useState<WsObserveClient | null>(null);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const nextClient = new WsObserveClient(
      obsUrl ? () => obsUrl : defaultObsUrl,
    );
    const offConnection = nextClient.onConnectionChange(setConnected);
    setClient(nextClient);
    return () => {
      offConnection();
      nextClient.dispose();
      setClient(null);
    };
  }, [obsUrl]);

  if (!client) return null;
  return <TelemetryView client={client} connected={connected} />;
};

function inferSourceKind(source: string): SourceKind {
  const lower = source.toLowerCase();
  if (lower.includes('test')) return 'test';
  if (lower.includes('playground')) return 'playground';
  if (lower.includes('baml')) return 'cli';
  return 'sdk';
}

const TelemetryView: FC<{ client: WsObserveClient; connected: boolean }> = ({
  client,
  connected,
}) => {
  const [liveRows, setLiveRows] = useState<ExecutionRow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [sourceFilter, setSourceFilter] = useState<SourceKind | 'all'>('all');
  const [problemsOnly, setProblemsOnly] = useState(false);

  useEffect(
    () =>
      client.subscribe('runs', {}, (frame) => {
        if (frame.kind !== FrameKind.RunsList) return;
        try {
          const rows = asRunsList(frame);
          setLiveRows(
            rows.runKey.map((id, index) => {
              const createdMs = rows.createdMs[index] ?? 0;
              const completedMs = rows.completedMs[index] ?? 0;
              const status = normalizeStatus(rows.status[index] ?? 'succeeded');
              const source = rows.source[index] || 'local runtime';
              return {
                calls: null,
                durationMs:
                  completedMs > createdMs ? completedMs - createdMs : null,
                entryPoint: source,
                errors: null,
                id,
                live: true,
                revision: rows.revision[index] || 'unknown',
                savedBytes: null,
                sourceKind: inferSourceKind(source),
                spanCount: null,
                startedMs: createdMs,
                status,
                target: rows.target[index] || 'parentless thread',
              };
            }),
          );
        } catch (error) {
          console.warn('Telemetry: bad executions frame', error);
        }
      }),
    [client],
  );

  const executions = useMemo(
    () =>
      [...liveRows, ...PROTOTYPE_EXECUTIONS].sort(
        (left, right) => right.startedMs - left.startedMs,
      ),
    [liveRows],
  );
  // Keep a stable snapshot so a live list refresh can't yank the open detail.
  const [selectedSnapshot, setSelectedSnapshot] = useState<ExecutionRow | null>(
    null,
  );
  const selected = selectedId
    ? (executions.find((row) => row.id === selectedId) ?? selectedSnapshot)
    : null;

  const visible = useMemo(() => {
    const query = search.trim().toLowerCase();
    return executions.filter((row) => {
      if (sourceFilter !== 'all' && row.sourceKind !== sourceFilter)
        return false;
      if (problemsOnly && row.status === 'succeeded' && (row.errors ?? 0) === 0)
        return false;
      return (
        !query ||
        `${row.target} ${row.entryPoint} ${row.status}`
          .toLowerCase()
          .includes(query)
      );
    });
  }, [executions, problemsOnly, search, sourceFilter]);

  const open = useCallback((row: ExecutionRow) => {
    setSelectedId(row.id);
    setSelectedSnapshot(row);
  }, []);
  const close = useCallback(() => {
    setSelectedId(null);
    setSelectedSnapshot(null);
  }, []);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-vsc-bg font-vsc text-vsc-text">
      <TelemetryHeader
        connected={connected}
        listView={!selected}
        search={search}
        setSearch={setSearch}
      />
      {selected ? (
        <ExecutionDetail client={client} execution={selected} onBack={close} />
      ) : (
        <ExecutionsList
          all={executions}
          onOpen={open}
          problemsOnly={problemsOnly}
          rows={visible}
          setProblemsOnly={setProblemsOnly}
          setSourceFilter={setSourceFilter}
          sourceFilter={sourceFilter}
        />
      )}
    </div>
  );
};

const TelemetryHeader: FC<{
  connected: boolean;
  listView: boolean;
  search: string;
  setSearch: (value: string) => void;
}> = ({ connected, listView, search, setSearch }) => (
  <header className="flex h-11 shrink-0 items-center gap-3 border-b border-vsc-border bg-vsc-surface px-3">
    <div className="flex items-center gap-2">
      <Activity className="h-4 w-4 text-vsc-accent" />
      <span className="text-[13px] font-semibold">Telemetry</span>
      <Pill className="border-vsc-green/20 bg-vsc-green/5 text-vsc-green">
        <Circle className="h-1.5 w-1.5 fill-current" /> local only
      </Pill>
    </div>
    <div className="ml-auto" />
    {listView && (
      <div className="relative w-56 max-w-[28vw]">
        <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-vsc-text-faint" />
        <input
          aria-label="Search executions"
          className="h-7 w-full rounded border border-vsc-input-border bg-vsc-input-bg pl-7 pr-2 text-[11px] text-vsc-input-fg outline-none placeholder:text-vsc-text-faint focus:border-vsc-accent"
          onChange={(event) => setSearch(event.target.value)}
          placeholder="Search functions, commands, status…"
          value={search}
        />
      </div>
    )}
    <button
      aria-label="Telemetry settings"
      className="rounded p-1.5 text-vsc-text-muted hover:bg-vsc-hover"
      type="button"
    >
      <Settings2 className="h-4 w-4" />
    </button>
    <span
      className={cn(
        'h-2 w-2 rounded-full',
        connected ? 'bg-vsc-green' : 'bg-vsc-yellow',
      )}
      title={
        connected
          ? 'Local profiler connected'
          : 'Local profiler not connected — example executions remain available'
      }
    />
  </header>
);

// ---------------------------------------------------------------------------
// Executions list
// ---------------------------------------------------------------------------

const SOURCE_FILTERS: Array<{ key: SourceKind | 'all'; label: string }> = [
  { key: 'all', label: 'All' },
  { key: 'cli', label: 'CLI' },
  { key: 'playground', label: 'Playground' },
  { key: 'test', label: 'Tests' },
  { key: 'sdk', label: 'SDK' },
];

const AggregateCard: FC<{
  icon: FC<{ className?: string }>;
  label: string;
  value: string;
  detail?: string;
  tone?: string;
}> = ({ icon: Icon, label, value, detail, tone }) => (
  <div className="flex min-w-0 items-center gap-2 rounded-md border border-vsc-border bg-vsc-surface px-3 py-2">
    <Icon className={cn('h-4 w-4 shrink-0 text-vsc-text-muted', tone)} />
    <div className="min-w-0">
      <div className="text-[9px] font-medium uppercase tracking-wide text-vsc-text-faint">
        {label}
      </div>
      <div className="mt-0.5 truncate font-vsc-mono text-[13px] font-semibold text-vsc-text-bright">
        {value}
      </div>
      {detail && (
        <div className="truncate text-[9px] text-vsc-text-faint">{detail}</div>
      )}
    </div>
  </div>
);

const ExecutionsList: FC<{
  all: ExecutionRow[];
  rows: ExecutionRow[];
  sourceFilter: SourceKind | 'all';
  setSourceFilter: (value: SourceKind | 'all') => void;
  problemsOnly: boolean;
  setProblemsOnly: (value: boolean) => void;
  onOpen: (row: ExecutionRow) => void;
}> = ({
  all,
  rows,
  sourceFilter,
  setSourceFilter,
  problemsOnly,
  setProblemsOnly,
  onOpen,
}) => {
  const problems = all.filter(
    (row) => row.status !== 'succeeded' || (row.errors ?? 0) > 0,
  ).length;
  const knownCalls = all.filter((row) => row.calls !== null);
  const totalCalls = knownCalls.reduce((sum, row) => sum + (row.calls ?? 0), 0);
  const knownSaved = all.filter((row) => row.savedBytes !== null);
  const savedBytes = knownSaved.reduce(
    (sum, row) => sum + (row.savedBytes ?? 0),
    0,
  );

  return (
    <main className="min-h-0 flex-1 overflow-auto">
      <div className="mx-auto w-full max-w-[1500px] p-4">
        <div className="flex items-end justify-between gap-4">
          <div>
            <h1 className="text-[15px] font-semibold text-vsc-text-bright">
              Executions
            </h1>
            <p className="mt-1 text-[11px] text-vsc-text-muted">
              Every recorded entry point — CLI runs, tests, playground calls,
              and SDK processes — newest first.
            </p>
          </div>
          <div className="flex items-center gap-3">
            <div className="flex items-center rounded border border-vsc-border p-0.5">
              {SOURCE_FILTERS.map(({ key, label }) => (
                <button
                  className={cn(
                    'rounded px-2 py-1 text-[10px]',
                    sourceFilter === key
                      ? 'bg-vsc-accent/15 font-medium text-vsc-accent'
                      : 'text-vsc-text-muted hover:text-vsc-text',
                  )}
                  key={key}
                  onClick={() => setSourceFilter(key)}
                  type="button"
                >
                  {label}
                </button>
              ))}
            </div>
            <label className="flex cursor-pointer items-center gap-1.5 text-[10px] text-vsc-text-muted">
              <input
                checked={problemsOnly}
                className="accent-vsc-accent"
                onChange={(event) => setProblemsOnly(event.target.checked)}
                type="checkbox"
              />
              problems only
            </label>
          </div>
        </div>

        <div className="mt-4 grid grid-cols-2 gap-2 md:grid-cols-4">
          <AggregateCard
            icon={Activity}
            label="Executions"
            value={all.length.toLocaleString()}
          />
          <AggregateCard
            icon={AlertCircle}
            label="Problems"
            tone={problems > 0 ? 'text-vsc-yellow' : 'text-vsc-green'}
            value={problems.toLocaleString()}
          />
          <AggregateCard
            detail={
              knownCalls.length < all.length
                ? `across ${knownCalls.length} of ${all.length} executions`
                : undefined
            }
            icon={Layers3}
            label="Function calls"
            value={totalCalls.toLocaleString()}
          />
          <AggregateCard
            detail="lossless — every pointer to identical content is one copy"
            icon={Database}
            label="Compressed away"
            tone="text-vsc-green"
            value={knownSaved.length > 0 ? formatBytes(savedBytes) : '—'}
          />
        </div>

        <div className="mt-3 overflow-hidden rounded-md border border-vsc-border bg-vsc-surface">
          <div className="overflow-x-auto">
            <table className="w-full min-w-[960px] border-collapse text-left">
              <thead className="border-b border-vsc-border bg-vsc-bg text-[9px] font-semibold uppercase tracking-wide text-vsc-text-faint">
                <tr>
                  <th className="w-[128px] px-3 py-2" scope="col">
                    Status
                  </th>
                  <th className="px-3 py-2" scope="col">
                    Execution
                  </th>
                  <th className="w-[112px] px-3 py-2" scope="col">
                    <span className="inline-flex items-center gap-1">
                      Started <ChevronDown className="h-3 w-3" />
                    </span>
                  </th>
                  <th className="w-[88px] px-3 py-2 text-right" scope="col">
                    Duration
                  </th>
                  <th className="w-[88px] px-3 py-2 text-right" scope="col">
                    Calls
                  </th>
                  <th className="w-[76px] px-3 py-2 text-right" scope="col">
                    Errors
                  </th>
                  <th className="w-[168px] px-3 py-2" scope="col">
                    Evidence
                  </th>
                  <th className="w-[68px] px-3 py-2" scope="col">
                    <span className="sr-only">Open</span>
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-vsc-border-subtle">
                {rows.map((row) => (
                  <ExecutionRowView key={row.id} onOpen={onOpen} row={row} />
                ))}
              </tbody>
            </table>
          </div>
          {rows.length === 0 && (
            <div className="px-4 py-10 text-center text-[11px] text-vsc-text-muted">
              No executions match the current search and filters.
            </div>
          )}
          <div className="flex items-center justify-between border-t border-vsc-border bg-vsc-bg px-3 py-2 text-[9px] text-vsc-text-faint">
            <span>
              Showing {rows.length} of {all.length} local executions
            </span>
            <span>
              Aggregate summaries are complete for every execution; spans are
              retained by policy and the recent exact window.
            </span>
          </div>
        </div>
      </div>
    </main>
  );
};

const ExecutionRowView: FC<{
  row: ExecutionRow;
  onOpen: (row: ExecutionRow) => void;
}> = ({ row, onOpen }) => (
  <tr className="group hover:bg-vsc-hover">
    <td className="px-3 py-2.5">
      <span
        className={cn(
          'inline-flex items-center gap-1.5 rounded border px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide',
          statusStyles(row.status),
        )}
      >
        <StatusIcon status={row.status} />
        {row.status}
      </span>
    </td>
    <td className="min-w-0 px-3 py-2.5">
      <button
        className="block max-w-full text-left"
        onClick={() => onOpen(row)}
        type="button"
      >
        <span className="block truncate font-vsc-mono text-[11px] font-semibold text-vsc-text-bright group-hover:text-vsc-accent">
          {row.target}
        </span>
        <span className="mt-0.5 flex items-center gap-1.5 truncate text-[9px] text-vsc-text-faint">
          <SourceIcon sourceKind={row.sourceKind} />
          <span className="truncate font-vsc-mono">{row.entryPoint}</span>
          <span>·</span>
          <span className="font-vsc-mono">{shortRevision(row.revision)}</span>
          {row.prototype && (
            <span className="uppercase tracking-wide text-vsc-accent">
              · example
            </span>
          )}
        </span>
      </button>
    </td>
    <td className="px-3 py-2.5 font-vsc-mono text-[10px] text-vsc-text-muted">
      {new Date(row.startedMs).toLocaleTimeString([], { hour12: false })}
    </td>
    <td className="px-3 py-2.5 text-right font-vsc-mono text-[10px] text-vsc-text-muted">
      {row.durationMs === null ? '—' : formatDuration(row.durationMs)}
    </td>
    <td className="px-3 py-2.5 text-right font-vsc-mono text-[10px] text-vsc-text-muted">
      {formatCount(row.calls)}
    </td>
    <td
      className={cn(
        'px-3 py-2.5 text-right font-vsc-mono text-[10px]',
        (row.errors ?? 0) > 0
          ? 'font-semibold text-vsc-red'
          : 'text-vsc-text-muted',
      )}
    >
      {formatCount(row.errors)}
    </td>
    <td className="px-3 py-2.5">
      <div className="flex items-center gap-1.5">
        {row.status === 'running' ? (
          <Pill className="text-vsc-accent">
            <Circle className="h-1.5 w-1.5 animate-pulse fill-current" />
            recording
          </Pill>
        ) : (
          <>
            {/* The runs frame carries no span count; only claim one when
                known (prototype rows / future wire) — never promise spans. */}
            {row.spanCount !== null && (
              <Pill className="border-vsc-accent/20 bg-vsc-accent/5 text-vsc-accent">
                {row.spanCount} spans
              </Pill>
            )}
            <Pill>
              <Sigma className="h-2.5 w-2.5" /> aggregates
            </Pill>
          </>
        )}
      </div>
    </td>
    <td className="px-3 py-2.5 text-right">
      <button
        aria-label={`Open ${row.target}`}
        className="inline-flex items-center gap-1 rounded px-2 py-1 text-[10px] font-medium text-vsc-accent hover:bg-vsc-accent/10"
        onClick={() => onOpen(row)}
        type="button"
      >
        Open <ChevronRight className="h-3 w-3" />
      </button>
    </td>
  </tr>
);

// ---------------------------------------------------------------------------
// Execution detail — shared selection state drives the Trace↔Profile pivots.
// ---------------------------------------------------------------------------

const ExecutionDetail: FC<{
  client: WsObserveClient;
  execution: ExecutionRow;
  onBack: () => void;
}> = ({ client, execution, onBack }) => {
  const [mode, setMode] = useState<ViewMode>('overview');
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);
  const [selectedContextId, setSelectedContextId] = useState<string | null>(
    null,
  );

  const liveEvidence = useLiveEvidence(
    client,
    execution.live ? execution : null,
  );
  const evidence = execution.evidence ?? liveEvidence;

  // The two pivots. Opening a span keeps its context selected (and vice
  // versa) so switching tabs preserves "the thing I was looking at".
  const openSpan = useCallback((span: SpanNode) => {
    setSelectedSpanId(span.id);
    if (span.contextId) setSelectedContextId(span.contextId);
    setMode('trace');
  }, []);
  const openContext = useCallback((contextId: string) => {
    setSelectedContextId(contextId);
    setMode('profile');
  }, []);

  const durationMs =
    execution.durationMs ??
    (evidence && evidence.spans.length > 0
      ? Math.max(...evidence.spans.map((s) => s.startMs + s.durationMs))
      : null);

  return (
    <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <DetailHeader
        execution={execution}
        mode={mode}
        onBack={onBack}
        setMode={setMode}
      />
      <div className="min-h-0 flex-1 overflow-auto">
        {execution.prototype && (
          <div className="mx-3 mt-3 flex items-center gap-2 rounded-md border border-vsc-accent/25 bg-vsc-accent/5 px-3 py-2 text-[11px] text-vsc-text-muted">
            <Sparkles className="h-3.5 w-3.5 shrink-0 text-vsc-accent" />
            <span>
              <strong className="font-medium text-vsc-text">Example.</strong>{' '}
              This execution is prototype data illustrating the full contract;
              live executions show exactly what the local profiler recorded.
            </span>
          </div>
        )}
        {!evidence ? (
          <EvidenceLoading execution={execution} />
        ) : mode === 'overview' ? (
          <OverviewTab
            durationMs={durationMs}
            evidence={evidence}
            execution={execution}
            openContext={openContext}
            openSpan={openSpan}
          />
        ) : mode === 'trace' ? (
          <TraceTab
            durationMs={durationMs}
            evidence={evidence}
            openContext={openContext}
            selectedSpanId={selectedSpanId}
            setSelectedSpanId={setSelectedSpanId}
          />
        ) : (
          <ProfileTab
            evidence={evidence}
            openSpan={openSpan}
            selectedContextId={selectedContextId}
            setSelectedContextId={setSelectedContextId}
          />
        )}
      </div>
    </main>
  );
};

const DetailHeader: FC<{
  execution: ExecutionRow;
  mode: ViewMode;
  onBack: () => void;
  setMode: (mode: ViewMode) => void;
}> = ({ execution, mode, onBack, setMode }) => (
  <div className="shrink-0 border-b border-vsc-border bg-vsc-bg">
    <div className="flex items-center gap-2 px-3 pb-1 pt-2">
      <button
        aria-label="Back to executions"
        className="mr-1 inline-flex h-6 items-center gap-1 rounded px-1.5 text-[10px] text-vsc-text-muted hover:bg-vsc-hover hover:text-vsc-text"
        onClick={onBack}
        type="button"
      >
        <ArrowLeft className="h-3.5 w-3.5" />
        Executions
      </button>
      <span className="h-4 w-px bg-vsc-border" />
      <StatusIcon
        className={statusStyles(execution.status).split(' ')[0]}
        status={execution.status}
      />
      <h1 className="truncate font-vsc-mono text-[13px] font-semibold">
        {execution.target}
      </h1>
      <span
        className={cn(
          'rounded border px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide',
          statusStyles(execution.status),
        )}
      >
        {execution.status}
      </span>
      <div className="ml-auto flex min-w-0 items-center gap-2 font-vsc-mono text-[10px] text-vsc-text-muted">
        <SourceIcon className="shrink-0" sourceKind={execution.sourceKind} />
        <span className="truncate" title={execution.entryPoint}>
          {execution.entryPoint}
        </span>
        <span>·</span>
        <span className="shrink-0">{shortRevision(execution.revision)}</span>
      </div>
    </div>
    <nav className="flex h-8 items-end gap-1 px-3">
      {(
        [
          ['overview', BarChart3, 'Overview'],
          ['trace', Waypoints, 'Trace'],
          ['profile', Flame, 'Profile'],
        ] as const
      ).map(([value, Icon, label]) => (
        <button
          className={cn(
            'flex h-8 items-center gap-1.5 border-b-2 px-2 text-[11px]',
            mode === value
              ? 'border-vsc-accent text-vsc-text'
              : 'border-transparent text-vsc-text-muted hover:text-vsc-text',
          )}
          key={value}
          onClick={() => setMode(value)}
          type="button"
        >
          <Icon className="h-3.5 w-3.5" />
          {label}
        </button>
      ))}
      <span className="ml-auto pb-1.5 text-[9px] text-vsc-text-faint">
        Trace is ordered by time · Profile is ordered by call path
      </span>
    </nav>
  </div>
);

const EvidenceLoading: FC<{ execution: ExecutionRow }> = ({ execution }) => (
  <div className="flex min-h-[320px] items-center justify-center p-6">
    <div className="w-full max-w-lg rounded-md border border-vsc-border bg-vsc-surface p-5">
      <div className="flex items-center gap-2">
        <Loader2 className="h-4 w-4 animate-spin text-vsc-accent" />
        <h2 className="text-[12px] font-semibold">
          Reading local artifacts for {execution.target}
        </h2>
      </div>
      <p className="mt-2 text-[11px] leading-5 text-vsc-text-muted">
        Aggregate summaries, thread lanes, and the recent exact window load from
        the local profiler. Nothing here is invented — panels stay empty until
        their evidence arrives.
      </p>
    </div>
  </div>
);

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

const MetricCard: FC<{
  icon: FC<{ className?: string }>;
  label: string;
  value: string;
  detail: ReactNode;
  tone?: string;
}> = ({ icon: Icon, label, value, detail, tone }) => (
  <div className="min-w-0 rounded-md border border-vsc-border bg-vsc-surface p-3">
    <div className="flex items-center gap-1.5 text-[10px] font-medium uppercase tracking-wide text-vsc-text-muted">
      <Icon className="h-3.5 w-3.5" />
      {label}
    </div>
    <div
      className={cn(
        'mt-2 font-vsc-mono text-xl font-semibold text-vsc-text-bright',
        tone,
      )}
    >
      {value}
    </div>
    <div className="mt-1 text-[10px] text-vsc-text-faint">{detail}</div>
  </div>
);

const OverviewTab: FC<{
  execution: ExecutionRow;
  evidence: Evidence;
  durationMs: number | null;
  openContext: (contextId: string) => void;
  openSpan: (span: SpanNode) => void;
}> = ({ execution, evidence, durationMs, openContext, openSpan }) => {
  const totalCalls =
    execution.calls ??
    evidence.contexts
      .filter((c) => !c.folded)
      .reduce((s, c) => s + c.enters, 0);
  const totalErrors =
    execution.errors ??
    evidence.contexts
      .filter((c) => !c.folded)
      .reduce((s, c) => s + c.errors, 0);
  const rootTotal = Math.max(
    1,
    ...evidence.contexts
      .filter((c) => c.parentId === null)
      .map((c) => c.totalMs),
  );
  const hotContexts = evidence.contexts
    .filter((c) => !c.folded && c.parentId !== null)
    .sort((a, b) => b.selfMs - a.selfMs)
    .slice(0, 4);
  const problemSpans = evidence.spans.filter((s) => s.status === 'failed');
  const reasons = new Map<string, number>();
  for (const span of evidence.spans)
    reasons.set(span.reason, (reasons.get(span.reason) ?? 0) + 1);

  return (
    <div className="space-y-3 p-3">
      <div className="grid grid-cols-4 gap-2">
        <MetricCard
          detail={
            evidence.threads.length > 1
              ? `${evidence.threads.length} logical threads`
              : 'single logical thread'
          }
          icon={Clock3}
          label="Wall time"
          value={durationMs === null ? '—' : formatDuration(durationMs)}
        />
        <MetricCard
          detail="aggregate summaries are complete for every call"
          icon={Layers3}
          label="Calls"
          value={totalCalls.toLocaleString()}
        />
        <MetricCard
          detail={
            totalErrors > 0
              ? 'includes handled errors'
              : 'no call errors observed'
          }
          icon={AlertCircle}
          label="Errors"
          tone={totalErrors > 0 ? 'text-vsc-red' : undefined}
          value={totalErrors.toLocaleString()}
        />
        <MetricCard
          detail="deduplicated by the content-addressed store"
          icon={Database}
          label="Compressed away"
          tone="text-vsc-green"
          value={
            execution.savedBytes === null
              ? '—'
              : formatBytes(execution.savedBytes)
          }
        />
      </div>

      <div className="grid grid-cols-[minmax(0,1.35fr)_minmax(280px,0.85fr)] gap-3">
        <Panel
          subtitle="aggregate self time by calling context — opens Profile"
          title="Where time went"
        >
          <div className="divide-y divide-vsc-border-subtle">
            {hotContexts.map((context) => {
              const percent = Math.min(
                100,
                Math.round((context.selfMs / rootTotal) * 100),
              );
              return (
                <button
                  className="grid w-full grid-cols-[minmax(180px,1fr)_110px_70px] items-center gap-3 px-3 py-2 text-left hover:bg-vsc-hover"
                  key={context.id}
                  onClick={() => openContext(context.id)}
                  type="button"
                >
                  <div className="flex min-w-0 items-center gap-1.5">
                    <KindGlyph fn={context.fn} kind={context.kind} />
                    <div className="min-w-0">
                      <div className="truncate font-vsc-mono text-[11px] font-medium">
                        {context.fn}
                      </div>
                      <div className="truncate text-[10px] text-vsc-text-faint">
                        ×{context.enters.toLocaleString()} calls
                        {context.errors > 0 &&
                          ` · ${context.errors.toLocaleString()} errors`}
                      </div>
                    </div>
                  </div>
                  <div>
                    <div className="mb-1 flex justify-between font-vsc-mono text-[10px]">
                      <span>{formatDuration(context.selfMs)}</span>
                      <span className="text-vsc-text-faint">{percent}%</span>
                    </div>
                    <div className="h-1 rounded bg-vsc-bg">
                      <div
                        className="h-full rounded bg-vsc-accent"
                        style={{ width: `${percent}%` }}
                      />
                    </div>
                  </div>
                  <span className="text-right text-[10px] text-vsc-accent">
                    Profile <ArrowRight className="ml-0.5 inline h-3 w-3" />
                  </span>
                </button>
              );
            })}
            {hotContexts.length === 0 && (
              <div className="px-3 py-6 text-center text-[10px] text-vsc-text-faint">
                Aggregates not received yet.
              </div>
            )}
          </div>
        </Panel>

        <Panel subtitle="Can I trust what I'm seeing?" title="Recording health">
          <div className="space-y-2 p-3 text-[10px]">
            <HealthRow
              detail={`${totalCalls.toLocaleString()} calls · every context`}
              label="Aggregate summaries"
              state="complete"
            />
            <HealthRow
              detail={
                evidence.spans.length > 0
                  ? Array.from(reasons.entries())
                      .map(([reason, count]) => `${count} ${reason}`)
                      .join(' · ')
                  : 'none retained'
              }
              label={`Retained spans (${evidence.spans.length})`}
              state={evidence.spans.length > 0 ? 'available' : 'bounded'}
            />
            <HealthRow
              detail={
                evidence.valuesWired
                  ? `${evidence.spans.filter((s) => s.args !== undefined || s.result !== undefined || s.error !== undefined).length} bodies in the local value store`
                  : 'value reads not wired over this connection yet'
              }
              label="Captured values"
              state={evidence.valuesWired ? 'available' : 'bounded'}
            />
            <HealthRow
              detail={
                evidence.gaps.length === 0
                  ? 'none declared'
                  : evidence.gaps
                      .map(
                        (gap) =>
                          `${gap.calls.toLocaleString()} calls (${gap.reason})`,
                      )
                      .join(' · ')
              }
              label="Summary-only work"
              state={evidence.gaps.length === 0 ? 'complete' : 'declared'}
            />
            {!evidence.exactContextJoin && (
              <HealthRow
                detail="spans joined to contexts by function identity"
                label="Span↔context join"
                state="bounded"
              />
            )}
          </div>
          <div className="border-t border-vsc-border-subtle px-3 py-2 text-[10px] text-vsc-text-faint">
            Execution status and recording completeness are independent axes.
          </div>
        </Panel>
      </div>

      <div className="grid grid-cols-[minmax(0,1.35fr)_minmax(280px,0.85fr)] gap-3">
        <Panel
          subtitle="failed retained calls — opens Trace at the span"
          title="Problems"
        >
          {problemSpans.length > 0 ? (
            <div className="divide-y divide-vsc-border-subtle">
              {problemSpans.map((span) => (
                <button
                  className="grid w-full grid-cols-[minmax(160px,0.8fr)_minmax(140px,1fr)_86px] items-center gap-3 px-3 py-2 text-left hover:bg-vsc-hover"
                  key={span.id}
                  onClick={() => openSpan(span)}
                  type="button"
                >
                  <span className="flex min-w-0 items-center gap-1.5">
                    <XCircle className="h-3 w-3 shrink-0 text-vsc-red" />
                    <span className="truncate font-vsc-mono text-[11px]">
                      {span.fn}
                    </span>
                  </span>
                  <span className="truncate text-[10px] text-vsc-text-muted">
                    {typeof span.error === 'object' && span.error !== null
                      ? String(
                          (span.error as { message?: unknown }).message ??
                            (span.error as { type?: unknown }).type ??
                            'error retained',
                        )
                      : 'error retained'}
                  </span>
                  <span className="text-right font-vsc-mono text-[10px] text-vsc-text-faint">
                    +{formatDuration(span.startMs)}
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <div className="px-3 py-6 text-center text-[10px] text-vsc-text-faint">
              {totalErrors > 0
                ? 'Errors were counted in aggregates but no failing call was individually retained — see Profile.'
                : 'No failures recorded.'}
            </div>
          )}
        </Panel>

        <Panel
          subtitle="logical threads · busy vs awaiting"
          title="Concurrency"
        >
          <div className="space-y-2.5 p-3">
            {evidence.threads.map((lane) => {
              const span = Math.max(1, lane.lastMs - lane.firstMs);
              const busyPct =
                lane.busyMs === null
                  ? null
                  : Math.min(100, Math.round((lane.busyMs / span) * 100));
              return (
                <div key={lane.id}>
                  <div className="mb-1 flex items-center justify-between text-[10px]">
                    <span className="truncate">{lane.name}</span>
                    <span className="ml-2 shrink-0 font-vsc-mono text-vsc-text-faint">
                      {formatDuration(span)}
                      {(lane.errors ?? 0) > 0 && (
                        <span className="ml-1 text-vsc-red">
                          · {lane.errors} err
                        </span>
                      )}
                    </span>
                  </div>
                  <div className="h-1.5 overflow-hidden rounded bg-vsc-bg">
                    {busyPct !== null ? (
                      <div
                        className="h-full rounded bg-vsc-accent"
                        style={{ width: `${busyPct}%` }}
                        title={`busy ${formatDuration(lane.busyMs ?? 0)} · awaiting ${formatDuration(lane.awaitMs ?? 0)}`}
                      />
                    ) : (
                      <div className="h-full w-full rounded bg-vsc-border-subtle" />
                    )}
                  </div>
                </div>
              );
            })}
            {evidence.threads.length === 0 && (
              <div className="py-4 text-center text-[10px] text-vsc-text-faint">
                Thread lanes not received yet.
              </div>
            )}
          </div>
        </Panel>
      </div>
    </div>
  );
};

const HealthRow: FC<{
  label: string;
  state: 'complete' | 'available' | 'bounded' | 'declared';
  detail: string;
}> = ({ label, state, detail }) => (
  <div className="flex items-center gap-2">
    {state === 'bounded' ? (
      <Info className="h-3.5 w-3.5 shrink-0 text-vsc-text-muted" />
    ) : state === 'declared' ? (
      <Sigma className="h-3.5 w-3.5 shrink-0 text-vsc-text-muted" />
    ) : (
      <CheckCircle2 className="h-3.5 w-3.5 shrink-0 text-vsc-green" />
    )}
    <span className="min-w-0 flex-1 truncate text-vsc-text">{label}</span>
    <span className="min-w-0 max-w-[55%] truncate text-right text-vsc-text-faint">
      {detail}
    </span>
  </div>
);

// ---------------------------------------------------------------------------
// Trace — time-ordered spans; aggregate-only work appears as gap chips.
// ---------------------------------------------------------------------------

interface TraceRow {
  kind: 'span' | 'gap';
  depth: number;
  span?: SpanNode;
  gap?: GapInfo;
}

function flattenTrace(evidence: Evidence): TraceRow[] {
  const byParent = new Map<string | null, SpanNode[]>();
  for (const span of evidence.spans) {
    const list = byParent.get(span.parentId) ?? [];
    list.push(span);
    byParent.set(span.parentId, list);
  }
  for (const list of byParent.values())
    list.sort((a, b) => a.startMs - b.startMs);
  const gapsByParent = new Map<string | null, GapInfo[]>();
  for (const gap of evidence.gaps) {
    const list = gapsByParent.get(gap.parentSpanId) ?? [];
    list.push(gap);
    gapsByParent.set(gap.parentSpanId, list);
  }

  const rows: TraceRow[] = [];
  const walk = (span: SpanNode, depth: number) => {
    rows.push({ depth, kind: 'span', span });
    for (const child of byParent.get(span.id) ?? []) walk(child, depth + 1);
    // Gaps render after the retained children: their position among the
    // children is unknown, so they get the last slot rather than a fake one.
    for (const gap of gapsByParent.get(span.id) ?? [])
      rows.push({ depth: depth + 1, gap, kind: 'gap' });
  };
  for (const root of byParent.get(null) ?? []) walk(root, 0);
  for (const gap of gapsByParent.get(null) ?? [])
    rows.push({ depth: 0, gap, kind: 'gap' });
  return rows;
}

const TraceTab: FC<{
  evidence: Evidence;
  durationMs: number | null;
  selectedSpanId: string | null;
  setSelectedSpanId: (id: string | null) => void;
  openContext: (contextId: string) => void;
}> = ({
  evidence,
  durationMs,
  selectedSpanId,
  setSelectedSpanId,
  openContext,
}) => {
  const rows = useMemo(() => flattenTrace(evidence), [evidence]);
  const totalMs = Math.max(
    durationMs ?? 0,
    ...evidence.spans.map((s) => s.startMs + s.durationMs),
    1,
  );
  const selectedSpan =
    evidence.spans.find((span) => span.id === selectedSpanId) ??
    evidence.spans[0] ??
    null;
  const contextOf = (span: SpanNode | null): ContextNode | null =>
    span?.contextId
      ? (evidence.contexts.find((c) => c.id === span.contextId) ?? null)
      : null;

  return (
    <div className="flex h-full min-h-[520px] min-w-0">
      <div className="flex min-w-0 flex-[1.4] flex-col border-r border-vsc-border">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-vsc-border-subtle bg-vsc-surface px-3 text-[10px] text-vsc-text-muted">
          <Waypoints className="h-3.5 w-3.5" />
          <span>Retained spans, in observed order</span>
          <Pill className="border-vsc-accent/20 text-vsc-accent">
            {evidence.spans.length} spans
          </Pill>
          {evidence.gaps.length > 0 && (
            <Pill>
              <Sigma className="h-2.5 w-2.5" />
              {evidence.gaps
                .reduce((sum, gap) => sum + gap.calls, 0)
                .toLocaleString()}{' '}
              calls summarized
            </Pill>
          )}
          <div className="ml-auto font-vsc-mono">
            0 — {formatDuration(totalMs)}
          </div>
        </div>
        <TraceRuler totalMs={totalMs} />
        <div className="min-h-0 flex-1 overflow-auto py-1">
          {rows.map((row) =>
            row.kind === 'span' && row.span ? (
              <SpanRowView
                depth={row.depth}
                key={row.span.id}
                onSelect={() => setSelectedSpanId(row.span!.id)}
                selected={selectedSpan?.id === row.span.id}
                span={row.span}
                totalMs={totalMs}
              />
            ) : row.gap ? (
              <GapChipRow
                depth={row.depth}
                gap={row.gap}
                key={row.gap.id}
                onOpenProfile={() => openContext(row.gap!.contextId)}
              />
            ) : null,
          )}
          {evidence.spans.length === 0 && (
            <div className="mx-3 mt-4 rounded border border-dashed border-vsc-border px-3 py-4 text-[10px] text-vsc-text-muted">
              <Sigma className="mb-1 h-3.5 w-3.5" />
              No individual calls were retained for this execution — the
              aggregate summaries are still complete. Use{' '}
              <span className="font-medium text-vsc-text">Profile</span> to see
              every call path with counts and timing distributions.
            </div>
          )}
        </div>
      </div>
      <SpanInspector
        context={contextOf(selectedSpan)}
        evidence={evidence}
        onOpenContext={openContext}
        span={selectedSpan}
      />
    </div>
  );
};

const TraceRuler: FC<{ totalMs: number }> = ({ totalMs }) => (
  <div className="grid shrink-0 grid-cols-[minmax(220px,40%)_minmax(220px,1fr)_76px] border-b border-vsc-border bg-vsc-surface px-2 py-1 text-[9px] font-semibold uppercase tracking-wide text-vsc-text-faint">
    <span>Span</span>
    <span className="relative block h-4">
      {[0, 0.25, 0.5, 0.75, 1].map((f) => (
        <span
          className="absolute top-0 -translate-x-1/2 font-vsc-mono normal-case"
          key={f}
          style={{ left: `${f * 100}%` }}
        >
          {f === 0 ? '0' : formatDuration(totalMs * f)}
        </span>
      ))}
    </span>
    <span className="text-right">Duration</span>
  </div>
);

const SpanRowView: FC<{
  span: SpanNode;
  depth: number;
  totalMs: number;
  selected: boolean;
  onSelect: () => void;
}> = ({ span, depth, totalMs, selected, onSelect }) => {
  const left = Math.min(98, (span.startMs / totalMs) * 100);
  const width = Math.max(
    0.6,
    Math.min(100 - left, (span.durationMs / totalMs) * 100),
  );
  return (
    <div
      className={cn(
        'group grid h-8 grid-cols-[minmax(220px,40%)_minmax(220px,1fr)_76px] items-center px-2 text-[10px] hover:bg-vsc-hover',
        selected && 'bg-vsc-accent/10',
      )}
    >
      <button
        className="flex min-w-0 items-center gap-1.5 text-left"
        onClick={onSelect}
        style={{ paddingLeft: depth * 16 }}
        type="button"
      >
        <KindGlyph fn={span.fn} kind={span.kind} />
        <span className="truncate font-vsc-mono">{span.fn}</span>
        {span.threadName !== 'main' && (
          <span className="truncate rounded bg-vsc-bg px-1 text-[9px] text-vsc-text-faint">
            {span.threadName}
          </span>
        )}
        {span.status === 'failed' && (
          <XCircle className="h-3 w-3 shrink-0 text-vsc-red" />
        )}
        {span.status === 'cancelled' && (
          <Pause className="h-3 w-3 shrink-0 text-vsc-yellow" />
        )}
      </button>
      <button
        className="relative h-5 overflow-hidden rounded-sm bg-vsc-surface"
        onClick={onSelect}
        type="button"
      >
        <span
          className={cn(
            'absolute inset-y-0 rounded-sm opacity-85',
            span.status === 'failed' && 'ring-1 ring-inset ring-vsc-red',
          )}
          style={{
            backgroundColor: functionColor(span.fn),
            left: `${left}%`,
            width: `${width}%`,
          }}
        />
      </button>
      <button
        className={cn(
          'pr-1 text-right font-vsc-mono',
          span.status === 'failed' ? 'text-vsc-red' : 'text-vsc-text-muted',
        )}
        onClick={onSelect}
        type="button"
      >
        {formatDuration(span.durationMs)}
      </button>
    </div>
  );
};

/**
 * A gap chip is the only way summary-only work appears in Trace: one honest
 * object with a count and a pivot — never bars, never placeholder rows
 * (honesty rules 1–3).
 */
const GapChipRow: FC<{
  gap: GapInfo;
  depth: number;
  onOpenProfile: () => void;
}> = ({ gap, depth, onOpenProfile }) => (
  <div className="grid grid-cols-[minmax(220px,40%)_minmax(220px,1fr)_76px] items-center px-2 py-0.5">
    <div />
    <button
      className="col-span-2 flex items-center gap-2 rounded border border-dashed border-vsc-border px-2 py-1 text-left text-[9px] text-vsc-text-muted hover:border-vsc-accent/40 hover:bg-vsc-hover"
      onClick={onOpenProfile}
      style={{ marginLeft: depth * 8 }}
      type="button"
    >
      <Sigma className="h-3 w-3 shrink-0" />
      <span className="min-w-0 flex-1 truncate">
        {gap.calls.toLocaleString()} more {gap.calls === 1 ? 'call' : 'calls'}
        {gap.functions.length > 0 && (
          <>
            {' '}
            across{' '}
            <span className="font-vsc-mono text-vsc-text">
              {gap.functions.slice(0, 3).join(', ')}
            </span>
            {gap.functions.length > 3 && ` +${gap.functions.length - 3} more`}
          </>
        )}{' '}
        —{' '}
        {gap.reason === 'window'
          ? 'aged out of the recent exact window; aggregates remain complete'
          : 'summarized by policy, not individually retained'}
      </span>
      <span className="shrink-0 text-vsc-accent">
        Aggregate <ArrowRight className="ml-0.5 inline h-3 w-3" />
      </span>
    </button>
  </div>
);

const UnavailableValue: FC<{
  valueRole: string;
  why: 'policy' | 'connection';
}> = ({ valueRole, why }) => (
  <div>
    <SectionHeading>{valueRole}</SectionHeading>
    <div className="rounded border border-dashed border-vsc-border p-2 text-[10px] text-vsc-text-faint">
      {why === 'policy'
        ? 'not captured — the capture policy did not select this call'
        : 'not readable over this connection yet — the body may exist in the local value store'}
    </div>
  </div>
);

const SpanInspector: FC<{
  span: SpanNode | null;
  context: ContextNode | null;
  evidence: Evidence;
  onOpenContext: (contextId: string) => void;
}> = ({ span, context, evidence, onOpenContext }) => {
  if (!span) {
    return (
      <aside className="flex min-w-[320px] flex-1 items-center justify-center bg-vsc-bg p-6 text-center text-[10px] text-vsc-text-faint">
        Select a span to inspect its exact evidence.
      </aside>
    );
  }
  const retainedForContext = context
    ? evidence.spans.filter((s) => s.contextId === context.id).length
    : 0;
  const hasBodies =
    span.args !== undefined ||
    span.result !== undefined ||
    span.error !== undefined;
  return (
    <aside className="flex min-w-[320px] flex-1 flex-col overflow-auto bg-vsc-bg">
      <div className="border-b border-vsc-border bg-vsc-surface px-3 py-3">
        <div className="flex items-start gap-2">
          <span className="mt-0.5">
            <KindGlyph fn={span.fn} kind={span.kind} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate font-vsc-mono text-[13px] font-semibold">
              {span.fn}
            </div>
            <div className="mt-1 flex flex-wrap gap-1.5">
              <Pill>{span.kind} call</Pill>
              <Pill>thread {span.threadName}</Pill>
              <Pill className="border-vsc-accent/20 text-vsc-accent">
                span · {span.reason}
              </Pill>
            </div>
          </div>
          <StatusIcon
            className={statusStyles(span.status).split(' ')[0]}
            status={span.status}
          />
        </div>
        <div className="mt-3 grid grid-cols-3 gap-2 rounded border border-vsc-border-subtle bg-vsc-bg p-2">
          <SmallCard
            label="Started"
            value={`+${formatDuration(span.startMs)}`}
          />
          <SmallCard label="Duration" value={formatDuration(span.durationMs)} />
          <SmallCard
            label="Self time"
            value={
              span.selfMs === undefined ? '—' : formatDuration(span.selfMs)
            }
          />
        </div>
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3">
        {!hasBodies && span.valuesUnavailable === 'policy' && (
          <div className="flex items-start gap-1.5 rounded border border-vsc-border-subtle bg-vsc-surface p-2 text-[9px] text-vsc-text-faint">
            <Info className="mt-0.5 h-3 w-3 shrink-0" />
            Structure and values are independent evidence. This call's timing,
            status, and caller are exact — that is what makes it a span. Its
            argument/return bodies were a separate capture decision that no
            policy selected, so no body exists to load.
          </div>
        )}
        {span.args !== undefined ? (
          <ValueBlock label="Arguments" value={span.args} />
        ) : (
          <UnavailableValue
            valueRole="Arguments"
            why={span.valuesUnavailable ?? 'policy'}
          />
        )}
        {span.result !== undefined ? (
          <ValueBlock label="Return" value={span.result} />
        ) : span.status === 'failed' ? null : (
          <UnavailableValue
            valueRole="Return"
            why={span.valuesUnavailable ?? 'policy'}
          />
        )}
        {span.error !== undefined && (
          <ValueBlock label="Error" tone="error" value={span.error} />
        )}
        {span.cvalues && span.cvalues.length > 0 && (
          <div>
            <SectionHeading>Captured values</SectionHeading>
            <div className="space-y-1">
              {span.cvalues.map((item) => (
                <div
                  className="flex items-center gap-2 rounded border border-vsc-border-subtle bg-vsc-surface px-2 py-1.5 text-[10px]"
                  key={item.name}
                >
                  <Braces className="h-3 w-3 text-vsc-accent" />
                  <span className="font-vsc-mono">{item.name}</span>
                  <span className="text-vsc-text-faint">{item.type}</span>
                  <span className="ml-auto font-vsc-mono text-vsc-text-bright">
                    {JSON.stringify(item.value)}
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
        {hasBodies &&
          (span.inputBytes !== undefined || span.outputBytes !== undefined) && (
            <div>
              <SectionHeading>Payload sizes</SectionHeading>
              <div className="grid grid-cols-2 gap-2">
                <SmallCard
                  label="Input"
                  value={
                    span.inputBytes === undefined
                      ? '—'
                      : formatBytes(span.inputBytes)
                  }
                />
                <SmallCard
                  label="Output"
                  value={
                    span.outputBytes === undefined
                      ? '—'
                      : formatBytes(span.outputBytes)
                  }
                />
              </div>
            </div>
          )}
      </div>

      {context && (
        <div className="border-t border-vsc-border bg-vsc-surface px-3 py-2.5">
          <SectionHeading>Calling context</SectionHeading>
          <div className="flex items-center gap-2 text-[10px] text-vsc-text-muted">
            <Sigma className="h-3 w-3 shrink-0" />
            <span className="min-w-0 flex-1 truncate">
              This path ran{' '}
              <span className="font-vsc-mono text-vsc-text">
                ×{context.enters.toLocaleString()}
              </span>{' '}
              in this execution · {retainedForContext} retained
              {!evidence.exactContextJoin && ' (joined by function identity)'}
            </span>
            <button
              className="shrink-0 text-vsc-accent hover:underline"
              onClick={() => onOpenContext(context.id)}
              type="button"
            >
              View in Profile <ArrowRight className="ml-0.5 inline h-3 w-3" />
            </button>
          </div>
        </div>
      )}
    </aside>
  );
};

// ---------------------------------------------------------------------------
// Profile — the CCT: flame graph + context tree, with exemplar pivots.
// ---------------------------------------------------------------------------

interface ContextTreeNode {
  context: ContextNode;
  children: ContextTreeNode[];
}

function buildContextTree(contexts: ContextNode[]): ContextTreeNode[] {
  const nodes = new Map<string, ContextTreeNode>();
  for (const context of contexts)
    nodes.set(context.id, { children: [], context });
  const roots: ContextTreeNode[] = [];
  for (const context of contexts) {
    const node = nodes.get(context.id)!;
    const parent = context.parentId ? nodes.get(context.parentId) : undefined;
    if (parent) parent.children.push(node);
    else roots.push(node);
  }
  // Left-heavy ordering: heaviest subtree first, folds last.
  const sortRec = (list: ContextTreeNode[]) => {
    list.sort((a, b) => {
      if (a.context.folded !== undefined) return 1;
      if (b.context.folded !== undefined) return -1;
      return b.context.totalMs - a.context.totalMs;
    });
    for (const node of list) sortRec(node.children);
  };
  sortRec(roots);
  return roots;
}

const ProfileTab: FC<{
  evidence: Evidence;
  selectedContextId: string | null;
  setSelectedContextId: (id: string | null) => void;
  openSpan: (span: SpanNode) => void;
}> = ({ evidence, selectedContextId, setSelectedContextId, openSpan }) => {
  const tree = useMemo(() => buildContextTree(evidence.contexts), [evidence]);
  const selected =
    evidence.contexts.find((context) => context.id === selectedContextId) ??
    evidence.contexts[0] ??
    null;
  const exemplarsOf = (context: ContextNode): SpanNode[] =>
    evidence.spans.filter((span) => span.contextId === context.id);

  return (
    <div className="flex h-full min-h-[520px] min-w-0">
      <div className="flex min-w-0 flex-[1.4] flex-col border-r border-vsc-border">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-vsc-border-subtle bg-vsc-surface px-3 text-[10px] text-vsc-text-muted">
          <Flame className="h-3.5 w-3.5" />
          <span>Calling contexts, ordered by time spent</span>
          <Pill>
            <Sigma className="h-2.5 w-2.5" /> complete over every call
          </Pill>
          <span className="ml-auto text-[9px] text-vsc-text-faint">
            Not a timeline — for observed order, use Trace
          </span>
        </div>

        {tree.length > 0 ? (
          <>
            <div className="shrink-0 border-b border-vsc-border-subtle bg-vsc-bg p-2">
              <FlameGraph
                onSelect={setSelectedContextId}
                roots={tree}
                selectedId={selected?.id ?? null}
              />
            </div>
            <div className="grid shrink-0 grid-cols-[minmax(200px,1fr)_64px_54px_72px_72px_84px_96px] border-b border-vsc-border bg-vsc-surface px-2 py-1 text-[9px] font-semibold uppercase tracking-wide text-vsc-text-faint">
              <span>Context</span>
              <span className="text-right">Calls</span>
              <span className="text-right">Errors</span>
              <span className="text-right">Total</span>
              <span className="text-right">Self</span>
              <span>Distribution</span>
              <span className="text-right">Evidence</span>
            </div>
            <div className="min-h-0 flex-1 overflow-auto py-1">
              {tree.map((root) => (
                <ContextTreeRows
                  depth={0}
                  exemplarsOf={exemplarsOf}
                  key={root.context.id}
                  node={root}
                  onSelect={setSelectedContextId}
                  selectedId={selected?.id ?? null}
                />
              ))}
            </div>
          </>
        ) : (
          <div className="m-3 rounded border border-dashed border-vsc-border px-3 py-4 text-[10px] text-vsc-text-muted">
            Aggregate summaries have not arrived for this execution yet.
          </div>
        )}
      </div>
      <ContextInspector
        context={selected}
        evidence={evidence}
        exemplars={selected ? exemplarsOf(selected) : []}
        openSpan={openSpan}
      />
    </div>
  );
};

/**
 * Left-heavy flame graph. Width encodes total time within the parent; the
 * x-axis carries no ordering meaning (rule 1) — the header says so.
 */
const FlameGraph: FC<{
  roots: ContextTreeNode[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}> = ({ roots, selectedId, onSelect }) => {
  const totalMs = Math.max(1, ...roots.map((root) => root.context.totalMs));
  return (
    <div className="space-y-px">
      {roots.map((root) => (
        <FlameNode
          fraction={root.context.totalMs / totalMs}
          key={root.context.id}
          node={root}
          onSelect={onSelect}
          selectedId={selectedId}
        />
      ))}
    </div>
  );
};

const FlameNode: FC<{
  node: ContextTreeNode;
  fraction: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
}> = ({ node, fraction, selectedId, onSelect }) => {
  const { context, children } = node;
  // Spawned subtrees overlap the parent's wall time; normalize so children
  // never overflow the parent's box.
  const childSum = children.reduce(
    (sum, child) => sum + child.context.totalMs,
    0,
  );
  const scale =
    childSum > context.totalMs && childSum > 0 ? context.totalMs / childSum : 1;
  return (
    <div style={{ width: `${Math.max(fraction * 100, 0.5)}%` }}>
      <button
        className={cn(
          'block h-[18px] w-full truncate rounded-[2px] px-1 text-left font-vsc-mono text-[9px] leading-[18px] text-white/95',
          context.folded !== undefined && 'opacity-40',
          context.spawn && 'ring-1 ring-inset ring-white/40',
          selectedId === context.id && 'ring-2 ring-inset ring-vsc-accent',
        )}
        onClick={() => onSelect(context.id)}
        style={{
          backgroundColor:
            context.folded !== undefined
              ? 'var(--vsc-text-faint, #777)'
              : functionColor(context.fn),
        }}
        title={`${context.fn} · ×${context.enters.toLocaleString()} · ${formatDuration(context.totalMs)} total`}
        type="button"
      >
        {fraction > 0.07 && context.fn}
      </button>
      {children.length > 0 && (
        <div className="mt-px flex gap-px">
          {children.map((child) => (
            <FlameNode
              fraction={
                context.totalMs > 0
                  ? (child.context.totalMs * scale) / context.totalMs
                  : 0
              }
              key={child.context.id}
              node={child}
              onSelect={onSelect}
              selectedId={selectedId}
            />
          ))}
        </div>
      )}
    </div>
  );
};

const ContextTreeRows: FC<{
  node: ContextTreeNode;
  depth: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
  exemplarsOf: (context: ContextNode) => SpanNode[];
}> = ({ node, depth, selectedId, onSelect, exemplarsOf }) => {
  const [collapsed, setCollapsed] = useState(false);
  const { context, children } = node;
  const exemplars = exemplarsOf(context);
  return (
    <>
      <div
        className={cn(
          'grid h-8 grid-cols-[minmax(200px,1fr)_64px_54px_72px_72px_84px_96px] items-center px-2 text-[10px] hover:bg-vsc-hover',
          selectedId === context.id && 'bg-vsc-accent/10',
        )}
      >
        <div
          className="flex min-w-0 items-center"
          style={{ paddingLeft: depth * 16 }}
        >
          <button
            aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${context.fn}`}
            className="mr-1 flex h-4 w-4 shrink-0 items-center justify-center"
            disabled={children.length === 0}
            onClick={() => setCollapsed((current) => !current)}
            type="button"
          >
            {children.length > 0 ? (
              collapsed ? (
                <ChevronRight className="h-3 w-3" />
              ) : (
                <ChevronDown className="h-3 w-3" />
              )
            ) : (
              <span className="h-px w-2 bg-vsc-border" />
            )}
          </button>
          <button
            className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
            onClick={() => onSelect(context.id)}
            type="button"
          >
            <KindGlyph fn={context.fn} kind={context.kind} />
            <span
              className={cn(
                'truncate font-vsc-mono',
                context.folded !== undefined && 'italic text-vsc-text-faint',
              )}
            >
              {context.fn}
            </span>
            {context.spawn && (
              <span className="rounded bg-vsc-bg px-1 text-[9px] text-vsc-text-faint">
                spawned
              </span>
            )}
          </button>
        </div>
        <button
          className="text-right font-vsc-mono text-vsc-text-muted"
          onClick={() => onSelect(context.id)}
          type="button"
        >
          ×{context.enters.toLocaleString()}
        </button>
        <span
          className={cn(
            'text-right font-vsc-mono',
            context.errors > 0
              ? 'font-semibold text-vsc-red'
              : 'text-vsc-text-faint',
          )}
        >
          {context.errors.toLocaleString()}
        </span>
        <span className="text-right font-vsc-mono text-vsc-text-muted">
          {formatDuration(context.totalMs)}
        </span>
        <span className="text-right font-vsc-mono text-vsc-text-muted">
          {formatDuration(context.selfMs)}
        </span>
        <span className="pl-2">
          {context.histogram ? (
            <HistSpark buckets={context.histogram} />
          ) : (
            <span className="text-[9px] text-vsc-text-faint">—</span>
          )}
        </span>
        <span className="text-right">
          {exemplars.length > 0 ? (
            <button
              className="rounded border border-vsc-accent/25 bg-vsc-accent/5 px-1.5 py-0.5 font-vsc-mono text-[9px] text-vsc-accent hover:bg-vsc-accent/15"
              onClick={() => onSelect(context.id)}
              title="Retained instances of this path — inspect exact evidence"
              type="button"
            >
              {exemplars.length} of {context.enters.toLocaleString()}
            </button>
          ) : (
            <span className="text-[9px] text-vsc-text-faint">
              aggregate only
            </span>
          )}
        </span>
      </div>
      {!collapsed &&
        children.map((child) => (
          <ContextTreeRows
            depth={depth + 1}
            exemplarsOf={exemplarsOf}
            key={child.context.id}
            node={child}
            onSelect={onSelect}
            selectedId={selectedId}
          />
        ))}
    </>
  );
};

function contextPath(evidence: Evidence, context: ContextNode): string[] {
  const path: string[] = [];
  let current: ContextNode | undefined = context;
  while (current) {
    path.unshift(current.fn);
    current = current.parentId
      ? evidence.contexts.find((c) => c.id === current!.parentId)
      : undefined;
  }
  return path;
}

const ContextInspector: FC<{
  context: ContextNode | null;
  evidence: Evidence;
  exemplars: SpanNode[];
  openSpan: (span: SpanNode) => void;
}> = ({ context, evidence, exemplars, openSpan }) => {
  if (!context) {
    return (
      <aside className="flex min-w-[320px] flex-1 items-center justify-center bg-vsc-bg p-6 text-center text-[10px] text-vsc-text-faint">
        Select a calling context to inspect its aggregate.
      </aside>
    );
  }
  const path = contextPath(evidence, context);
  return (
    <aside className="flex min-w-[320px] flex-1 flex-col overflow-auto bg-vsc-bg">
      <div className="border-b border-vsc-border bg-vsc-surface px-3 py-3">
        <div className="flex items-start gap-2">
          <span className="mt-0.5">
            <KindGlyph fn={context.fn} kind={context.kind} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate font-vsc-mono text-[13px] font-semibold">
              {context.fn}
            </div>
            <div className="mt-1 truncate font-vsc-mono text-[9px] text-vsc-text-faint">
              {path.join(' → ')}
            </div>
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              <Pill>
                <Sigma className="h-2.5 w-2.5" /> aggregate
              </Pill>
              {context.spawn && <Pill>spawned thread</Pill>}
              {context.llm && <Pill>{context.llm.model}</Pill>}
            </div>
          </div>
        </div>
        <div className="mt-3 grid grid-cols-3 gap-2 rounded border border-vsc-border-subtle bg-vsc-bg p-2">
          <SmallCard
            label="Calls"
            value={`×${context.enters.toLocaleString()}`}
          />
          <SmallCard
            label="Errors"
            tone={context.errors > 0 ? 'text-vsc-red' : undefined}
            value={context.errors.toLocaleString()}
          />
          <SmallCard label="Total" value={formatDuration(context.totalMs)} />
          <SmallCard label="Self" value={formatDuration(context.selfMs)} />
          <SmallCard
            label="Await"
            value={
              context.awaitMs === null ? '—' : formatDuration(context.awaitMs)
            }
          />
          <SmallCard
            label="Mean / call"
            value={formatDuration(
              context.totalMs / Math.max(1, context.enters),
            )}
          />
        </div>
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3">
        {context.histogram && (
          <div>
            <SectionHeading>Duration distribution</SectionHeading>
            <div className="rounded border border-vsc-border-subtle bg-vsc-surface p-2">
              <div className="flex h-16 items-end gap-0.5">
                {context.histogram.map((count, index) => (
                  <span
                    className="min-w-0 flex-1 rounded-t-sm bg-vsc-accent/55"
                    // biome-ignore lint/suspicious/noArrayIndexKey: buckets are positional
                    key={index}
                    style={{
                      height: `${Math.max(count > 0 ? 6 : 2, (count / Math.max(1, ...context.histogram!)) * 100)}%`,
                    }}
                  />
                ))}
              </div>
              <div className="mt-1 flex justify-between font-vsc-mono text-[9px] text-vsc-text-faint">
                <span>fast</span>
                <span>
                  {context.enters.toLocaleString()} calls, bucketed by duration
                </span>
                <span>slow</span>
              </div>
            </div>
          </div>
        )}

        {context.llm && (
          <div>
            <SectionHeading>LLM totals</SectionHeading>
            <div className="grid grid-cols-2 gap-2">
              <SmallCard
                label="Tokens in"
                value={context.llm.tokensIn.toLocaleString()}
              />
              <SmallCard
                label="Tokens out"
                value={context.llm.tokensOut.toLocaleString()}
              />
            </div>
          </div>
        )}

        <div>
          <SectionHeading>
            Exemplars — {exemplars.length} of {context.enters.toLocaleString()}{' '}
            retained
          </SectionHeading>
          {exemplars.length > 0 ? (
            <div className="space-y-1">
              {exemplars.map((span) => (
                <button
                  className="flex w-full items-center gap-2 rounded border border-vsc-border-subtle bg-vsc-surface px-2 py-1.5 text-left text-[10px] hover:border-vsc-accent/40 hover:bg-vsc-hover"
                  key={span.id}
                  onClick={() => openSpan(span)}
                  type="button"
                >
                  <StatusIcon
                    className={statusStyles(span.status).split(' ')[0]}
                    status={span.status}
                  />
                  <span className="font-vsc-mono text-vsc-text-muted">
                    +{formatDuration(span.startMs)}
                  </span>
                  <span className="font-vsc-mono">
                    {formatDuration(span.durationMs)}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-vsc-text-faint">
                    {span.reason}
                  </span>
                  <span className="shrink-0 text-vsc-accent">
                    Trace <ArrowRight className="ml-0.5 inline h-3 w-3" />
                  </span>
                </button>
              ))}
              {exemplars.length < context.enters && (
                <div className="rounded border border-dashed border-vsc-border-subtle px-2 py-1.5 text-[9px] text-vsc-text-faint">
                  The other{' '}
                  {(context.enters - exemplars.length).toLocaleString()} calls
                  exist only in this aggregate — counts and timing above cover
                  all of them.
                </div>
              )}
            </div>
          ) : (
            <div className="rounded border border-dashed border-vsc-border p-2 text-[10px] text-vsc-text-faint">
              No individual calls retained for this path. Counts and timing
              above still cover every call. Pass{' '}
              <span className="font-vsc-mono text-vsc-text">$id</span> or widen
              the capture policy to retain future instances.
            </div>
          )}
        </div>

        {!evidence.exactContextJoin && exemplars.length > 0 && (
          <div className="flex items-start gap-1.5 rounded border border-vsc-border-subtle bg-vsc-surface p-2 text-[9px] text-vsc-text-faint">
            <Info className="mt-0.5 h-3 w-3 shrink-0" />
            Live spans are joined to contexts by function identity until the
            recent-calls frame carries the CCT node id; shared helpers may
            attribute to the wrong path.
          </div>
        )}

        <div className="flex items-start gap-1.5 rounded border border-vsc-border-subtle bg-vsc-surface p-2 text-[9px] text-vsc-text-faint">
          <Zap className="mt-0.5 h-3 w-3 shrink-0" />
          Aggregates are flat in time: they cannot say when, in what order, or
          under which parent instance these calls ran. Exact ordering lives in
          Trace, on retained spans only.
        </div>
      </div>
    </aside>
  );
};
