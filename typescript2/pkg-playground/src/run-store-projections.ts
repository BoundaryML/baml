import { decodeCallResult } from '@b/pkg-proto';
import type { BamlJsValue } from '@b/pkg-proto';

import type {
  FetchLogEntry,
  PayloadEvent,
  Run,
  RunId,
  RunTarget,
} from './worker-protocol';

export type RunStoreDisplayRun = {
  id: RunId;
  kind: RunTarget['kind'];
  projectId: string;
  projectGeneration: number;
  functionName: string;
  testName?: string;
  argsJson: string;
  fetchLogs: FetchLogEntry[];
  inputRequests: Array<{ id: string; prompt: string | null }>;
  result: BamlJsValue | null;
  error: string | null;
  status: 'running' | 'success' | 'error' | 'cancelled';
  startTime: number;
  durationMs: number | null;
};

export type RunTraceRow = {
  id: string;
  depth: number;
  functionName: string;
  status: Run['calls'][number]['status'];
  offsetMs: number | null;
  durationMs: number | null;
  spanLeftPct: number;
  spanWidthPct: number;
  sourceLine: number | null;
};

export type RunFlamegraphRow = {
  id: string;
  depth: number;
  functionName: string;
  status: Run['calls'][number]['status'];
  durationMs: number | null;
  selfMs: number | null;
  spanLeftPct: number;
  spanWidthPct: number;
};

export function runToDisplayRun(
  run: Run,
  argsJsonByRunId: Record<string, string>,
): RunStoreDisplayRun | null {
  const identity = runDisplayIdentity(run);
  if (!identity) return null;
  return {
    id: run.runId,
    kind: run.target.kind,
    projectId: run.request.projectId,
    projectGeneration: run.request.projectGeneration,
    functionName: identity.functionName,
    testName: identity.testName,
    argsJson: argsJsonByRunId[run.runId] ?? run.request.argsSummary ?? '',
    fetchLogs: payloadsToFetchLogs(run.payloads),
    inputRequests: payloadsToPendingInputs(run.payloads),
    result: decodeRunResultValue(run),
    error: run.error?.message ?? null,
    status: runStatusToDisplayStatus(run.status),
    startTime: run.startedAtMs ?? run.createdAtMs,
    durationMs: runDurationMs(run),
  };
}

export function runToTraceRows(run: Run | null | undefined): RunTraceRow[] {
  if (!run || run.calls.length === 0) return [];
  const calls = [...run.calls].sort(compareCallsByStart);
  const callsById = new Map(run.calls.map((call) => [call.id, call]));
  const starts = calls
    .map((call) => parseNs(call.startedAtNs))
    .filter((value): value is bigint => value !== null);
  const zeroNs = starts.length > 0 ? minBigInt(starts) : 0n;
  const maxNs = calls.reduce((max, call) => {
    const end = parseNs(call.endedAtNs) ?? parseNs(call.startedAtNs) ?? zeroNs;
    return end > max ? end : max;
  }, zeroNs);
  const spanNs = maxNs > zeroNs ? maxNs - zeroNs : 1n;

  return calls.map((call) => {
    const startNs = parseNs(call.startedAtNs);
    const endNs = parseNs(call.endedAtNs);
    const offsetNs = startNs !== null ? startNs - zeroNs : null;
    const durationNs =
      startNs !== null && endNs !== null && endNs >= startNs
        ? endNs - startNs
        : null;
    const leftPct =
      offsetNs !== null ? clampPercent((Number(offsetNs) / Number(spanNs)) * 100) : 0;
    const widthPct =
      durationNs !== null
        ? Math.max(1.5, clampPercent((Number(durationNs) / Number(spanNs)) * 100))
        : 1.5;
    return {
      id: call.id,
      depth: callDepth(call, callsById),
      functionName: call.functionName ?? `function#${call.functionId}`,
      status: call.status,
      offsetMs: offsetNs !== null ? Number(offsetNs) / 1_000_000 : null,
      durationMs: durationNs !== null ? Number(durationNs) / 1_000_000 : null,
      spanLeftPct: leftPct,
      spanWidthPct: Math.min(widthPct, 100 - leftPct),
      sourceLine: call.callSiteSource?.line ?? call.calleeSource?.line ?? null,
    };
  });
}

export function runToFlamegraphRows(
  run: Run | null | undefined,
): RunFlamegraphRow[] {
  if (!run || run.calls.length === 0) return [];
  const callsById = new Map(run.calls.map((call) => [call.id, call]));
  const childrenByParent = new Map<string | null, Run['calls']>();
  for (const call of run.calls) {
    const parentId = call.parentId && callsById.has(call.parentId) ? call.parentId : null;
    const children = childrenByParent.get(parentId);
    if (children) {
      children.push(call);
    } else {
      childrenByParent.set(parentId, [call]);
    }
  }
  for (const children of childrenByParent.values()) {
    children.sort(compareCallsByStart);
  }

  const starts = run.calls
    .map((call) => parseNs(call.startedAtNs))
    .filter((value): value is bigint => value !== null);
  const zeroNs = starts.length > 0 ? minBigInt(starts) : 0n;
  const maxNs = run.calls.reduce((max, call) => {
    const end = parseNs(call.endedAtNs) ?? parseNs(call.startedAtNs) ?? zeroNs;
    return end > max ? end : max;
  }, zeroNs);
  const spanNs = maxNs > zeroNs ? maxNs - zeroNs : 1n;
  const rows: RunFlamegraphRow[] = [];
  const visit = (call: Run['calls'][number], depth: number) => {
    rows.push(callToFlamegraphRow(call, depth, childrenByParent, zeroNs, spanNs));
    for (const child of childrenByParent.get(call.id) ?? []) {
      visit(child, depth + 1);
    }
  };

  const preferredRoot =
    run.rootCallNodeId != null ? callsById.get(run.rootCallNodeId) : undefined;
  const roots = childrenByParent.get(null) ?? [];
  if (preferredRoot) {
    visit(preferredRoot, 0);
    for (const root of roots) {
      if (root.id !== preferredRoot.id) visit(root, 0);
    }
  } else {
    for (const root of roots) {
      visit(root, 0);
    }
  }
  return rows;
}

function callToFlamegraphRow(
  call: Run['calls'][number],
  depth: number,
  childrenByParent: Map<string | null, Run['calls']>,
  zeroNs: bigint,
  spanNs: bigint,
): RunFlamegraphRow {
  const startNs = parseNs(call.startedAtNs);
  const endNs = parseNs(call.endedAtNs);
  const offsetNs = startNs !== null ? startNs - zeroNs : null;
  const durationNs =
    startNs !== null && endNs !== null && endNs >= startNs ? endNs - startNs : null;
  const childDurationNs =
    durationNs !== null
      ? (childrenByParent.get(call.id) ?? []).reduce((total, child) => {
          const childStart = parseNs(child.startedAtNs);
          const childEnd = parseNs(child.endedAtNs);
          if (childStart === null || childEnd === null || childEnd < childStart) {
            return total;
          }
          return total + (childEnd - childStart);
        }, 0n)
      : null;
  const selfNs =
    durationNs !== null && childDurationNs !== null
      ? durationNs > childDurationNs
        ? durationNs - childDurationNs
        : 0n
      : null;
  const leftPct =
    offsetNs !== null ? clampPercent((Number(offsetNs) / Number(spanNs)) * 100) : 0;
  const widthPct =
    durationNs !== null
      ? Math.max(1.5, clampPercent((Number(durationNs) / Number(spanNs)) * 100))
      : 1.5;

  return {
    id: call.id,
    depth,
    functionName: call.functionName ?? `function#${call.functionId}`,
    status: call.status,
    durationMs: durationNs !== null ? Number(durationNs) / 1_000_000 : null,
    selfMs: selfNs !== null ? Number(selfNs) / 1_000_000 : null,
    spanLeftPct: leftPct,
    spanWidthPct: Math.min(widthPct, 100 - leftPct),
  };
}

function runDisplayIdentity(
  run: Run,
): { functionName: string; testName?: string } | null {
  switch (run.target.kind) {
    case 'function':
      return { functionName: run.target.functionName };
    case 'test':
      return {
        functionName: 'testing.run_test',
        testName: run.target.testName,
      };
    case 'preview':
    case 'companion':
    case 'internal':
      return null;
    default:
      run.target satisfies never;
      return null;
  }
}

function compareCallsByStart(a: Run['calls'][number], b: Run['calls'][number]): number {
  const aStart = parseNs(a.startedAtNs);
  const bStart = parseNs(b.startedAtNs);
  if (aStart !== null && bStart !== null && aStart !== bStart) {
    return aStart < bStart ? -1 : 1;
  }
  if (aStart !== null && bStart === null) return -1;
  if (aStart === null && bStart !== null) return 1;
  return a.id.localeCompare(b.id);
}

function parseNs(value: string | null): bigint | null {
  if (value == null) return null;
  try {
    return BigInt(value);
  } catch {
    return null;
  }
}

function minBigInt(values: bigint[]): bigint {
  return values.reduce((min, value) => (value < min ? value : min));
}

function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(100, Math.max(0, value));
}

function callDepth(
  call: Run['calls'][number],
  callsById: Map<string, Run['calls'][number]>,
): number {
  let depth = 0;
  const seen = new Set<string>([call.id]);
  let parentId = call.parentId;
  while (parentId && depth < 32 && !seen.has(parentId)) {
    const parent = callsById.get(parentId);
    if (!parent) break;
    seen.add(parentId);
    depth += 1;
    parentId = parent.parentId;
  }
  return depth;
}

function runStatusToDisplayStatus(
  status: Run['status'],
): RunStoreDisplayRun['status'] {
  switch (status) {
    case 'succeeded':
      return 'success';
    case 'failed':
    case 'panicked':
      return 'error';
    case 'cancelled':
      return 'cancelled';
    case 'pending':
    case 'running':
    case 'waitingForInput':
    case 'waitingForEnv':
    case 'cancelling':
      return 'running';
    default:
      status satisfies never;
      return 'running';
  }
}

function runDurationMs(run: Run): number | null {
  const start = run.startedAtMs ?? run.createdAtMs;
  if (run.completedAtMs == null) return null;
  return Math.max(0, run.completedAtMs - start);
}

function payloadsToPendingInputs(
  payloads: PayloadEvent[],
): Array<{ id: string; prompt: string | null }> {
  const pending = new Map<string, { id: string; prompt: string | null }>();
  for (const payload of payloads) {
    const kind = payload.kind;
    if (kind.type === 'inputRequested') {
      if (kind.state === 'pending') {
        pending.set(kind.requestId, {
          id: kind.requestId,
          prompt: kind.prompt,
        });
      }
      continue;
    }
    if (kind.type === 'inputResolved') {
      pending.delete(kind.requestId);
    }
  }
  return [...pending.values()];
}

export function decodeRunResultValue(run: Run): BamlJsValue | null {
  const encoded = run.result?.value;
  if (!encoded) return null;
  try {
    const binary = atob(encoded);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    return decodeCallResult(bytes, (key, handleType, typeName) => ({
      handle_key: key,
      handle_type: handleType,
      type_name: typeName,
    }));
  } catch {
    return null;
  }
}

function payloadsToFetchLogs(payloads: PayloadEvent[]): FetchLogEntry[] {
  const logs = new Map<string, FetchLogEntry>();
  for (const payload of payloads) {
    const kind = payload.kind;
    if (kind.type === 'fetchStarted') {
      logs.set(kind.fetchId, {
        id: Number(kind.fetchId),
        timestamp: payload.timestampMs,
        method: kind.method,
        url: kind.url,
        requestHeaders: headersToRecord(kind.requestHeaders),
        requestBody: '',
        status: null,
        responseBody: null,
        error: null,
        durationMs: null,
        responseHeaders: null,
      });
      continue;
    }
    if (kind.type !== 'fetchUpdated') continue;

    const existing = logs.get(kind.fetchId);
    logs.set(kind.fetchId, {
      id: Number(kind.fetchId),
      timestamp: existing?.timestamp ?? payload.timestampMs,
      method: existing?.method ?? '',
      url: existing?.url ?? '',
      requestHeaders: existing?.requestHeaders ?? {},
      requestBody: existing?.requestBody ?? '',
      status: kind.status,
      responseBody: existing?.responseBody ?? null,
      error: kind.error,
      durationMs: kind.durationMs,
      responseHeaders: headersToRecord(kind.responseHeaders),
    });
  }
  return [...logs.values()].sort((a, b) => a.timestamp - b.timestamp);
}

function headersToRecord(
  headers: Array<{ name: string; valueRedacted: boolean }>,
): Record<string, string> {
  return Object.fromEntries(
    headers.map((header) => [
      header.name,
      header.valueRedacted ? '<redacted>' : '',
    ]),
  );
}
