import { decodeCallResult } from '@b/pkg-proto';
import type { BamlJsValue } from '@b/pkg-proto';

import type {
  FetchLogEntry,
  GraphRuntimeOverlay,
  PayloadEvent,
  Run,
  BoundaryId,
  RunTarget,
  ValueRef,
} from './worker-protocol';
import type { ValueBodyCache } from './value-body-cache';

export type RunStoreDisplayRun = {
  id: BoundaryId;
  kind: RunTarget['kind'];
  projectId: string;
  projectGeneration: number;
  functionName: string;
  testName?: string;
  argsJson: string;
  fetchLogs: FetchLogEntry[];
  outputChunks: RunOutputChunk[];
  inputRequests: Array<{ id: string; prompt: string | null }>;
  rootInput: BamlJsValue | null;
  result: BamlJsValue | null;
  error: string | null;
  errorValue: BamlJsValue | null;
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
  logs: RunTraceLog[];
  callValues: RunTraceCallValue[];
};

export type RunTraceLogState =
  | 'loading'
  | 'available'
  | 'pending'
  | 'omitted'
  | 'truncated'
  | 'missing'
  | 'lost'
  | 'error'
  | 'unavailable';

export type RunTraceLog = {
  id: string;
  timestampMs: number;
  level: string | null;
  message: string;
  sourceLine: number | null;
  valueRef: ValueRef | null;
  value: BamlJsValue | null;
  state: RunTraceLogState;
  diagnostic: string | null;
};

type LogPayloadEvent = PayloadEvent & {
  kind: Extract<PayloadEvent['kind'], { type: 'log' }>;
};

/**
 * One `baml.io` stream write, exactly as the VM produced it.
 *
 * Deliberately not split into lines or otherwise reshaped. The text may carry
 * ANSI escape sequences, and a single sequence can straddle two chunks, so the
 * only safe consumer is a terminal emulator fed the chunks in order.
 */
export type RunOutputChunk = {
  id: string;
  stream: 'stdout' | 'stderr';
  text: string;
  timestampMs: number;
};

type OutputPayloadEvent = PayloadEvent & {
  kind: Extract<PayloadEvent['kind'], { type: 'output' }>;
};

export type RunTraceCallValueRole = 'callInput' | 'callOutput' | 'callError';

export type RunTraceCallValue = {
  id: string;
  timestampMs: number;
  role: RunTraceCallValueRole;
  label: string | null;
  valueRef: ValueRef | null;
  value: BamlJsValue | null;
  state: RunTraceLogState;
  diagnostic: string | null;
};

type CapturedValuePayloadKind = Extract<
  PayloadEvent['kind'],
  { type: 'capturedValue' }
>;

type CallValuePayloadEvent = PayloadEvent & {
  kind: CapturedValuePayloadKind & { role: RunTraceCallValueRole };
};

type RootInputPayloadEvent = PayloadEvent & {
  kind: CapturedValuePayloadKind & { role: 'rootInput' };
};

export type GraphNodeValuePreview = RunTraceCallValue;

export type RunToGraphNodeValuesOptions = {
  rootGraphNodeId?: string | null;
};

export type ExecutionProfileOrigin = 'user' | 'library' | 'system' | 'unknown';

export type ExecutionProfileColorMode = 'function' | 'origin' | 'thread';

export type ExecutionProfileBlock = {
  id: string;
  threadId: string;
  threadLabel: string;
  parentId: string | null;
  depth: number;
  functionKey: string;
  functionName: string;
  origin: ExecutionProfileOrigin;
  status: Run['calls'][number]['status'];
  durationMs: number | null;
  selfMs: number | null;
  spanLeftPct: number;
  spanWidthPct: number;
  isSystemFrame: boolean;
};

export type ExecutionProfileFunctionRow = {
  functionKey: string;
  functionName: string;
  origin: ExecutionProfileOrigin;
  callCount: number;
  selfMs: number;
  totalMs: number;
};

export type ExecutionProfileProjection = {
  blocks: ExecutionProfileBlock[];
  functionRows: ExecutionProfileFunctionRow[];
  maxSelfMs: number;
  maxTotalMs: number;
  totalDurationMs: number | null;
};

export type ExecutionProfileFilters = {
  includeSystemCalls: boolean;
};

export function runToDisplayRun(
  run: Run,
  argsJsonByBoundaryId: Record<string, string>,
  valueBodyCache?: ValueBodyCache,
): RunStoreDisplayRun | null {
  const identity = runDisplayIdentity(run);
  if (!identity) return null;
  return {
    id: run.boundaryId,
    kind: run.target.kind,
    projectId: run.request.projectId,
    projectGeneration: run.request.projectGeneration,
    functionName: identity.functionName,
    testName: identity.testName,
    argsJson: argsJsonByBoundaryId[run.boundaryId] ?? run.request.argsSummary ?? '',
    fetchLogs: payloadsToFetchLogs(run.payloads),
    outputChunks: runToOutputChunks(run),
    inputRequests: payloadsToPendingInputs(run.payloads),
    rootInput: decodeRootInputValue(run, valueBodyCache),
    result: decodeRunResultValue(run, valueBodyCache),
    error: run.error?.message ?? null,
    errorValue: decodeRunErrorValue(run, valueBodyCache),
    status: runStatusToDisplayStatus(run.status),
    startTime: run.startedAtMs ?? run.createdAtMs,
    durationMs: runDurationMs(run),
  };
}

export function runToTraceRows(
  run: Run | null | undefined,
  valueBodyCache?: ValueBodyCache,
): RunTraceRow[] {
  if (!run || run.calls.length === 0) return [];
  const calls = [...run.calls].sort(compareCallsByStart);
  const callsById = new Map(run.calls.map((call) => [call.id, call]));
  const threadParentCallIds = threadParentCallIdsByThread(run, callsById);
  const logsByCallId = traceLogsByCallId(run, valueBodyCache);
  const callValuesByCallId = traceCallValuesByCallId(run, valueBodyCache);
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
      depth: callDepth(call, callsById, threadParentCallIds),
      functionName: call.functionName ?? `function#${call.functionId}`,
      status: call.status,
      offsetMs: offsetNs !== null ? Number(offsetNs) / 1_000_000 : null,
      durationMs: durationNs !== null ? Number(durationNs) / 1_000_000 : null,
      spanLeftPct: leftPct,
      spanWidthPct: Math.min(widthPct, 100 - leftPct),
      sourceLine: call.callSiteSource?.line ?? call.calleeSource?.line ?? null,
      logs: logsByCallId.get(call.id) ?? [],
      callValues: callValuesByCallId.get(call.id) ?? [],
    };
  });
}

export function runToGraphNodeValues(
  run: Run | null | undefined,
  overlay: GraphRuntimeOverlay | null | undefined,
  valueBodyCache?: ValueBodyCache,
  options: RunToGraphNodeValuesOptions = {},
): Map<string, GraphNodeValuePreview[]> {
  const valuesByNodeId = new Map<string, GraphNodeValuePreview[]>();
  if (!run) {
    return valuesByNodeId;
  }

  const nodeIdsByCallId = new Map<string, string[]>();
  if (overlay && overlay.entries.length > 0) {
    for (const entry of overlay.entries) {
      const nodeId = String(entry.cfgNodeId);
      for (const callNodeId of entry.callNodeIds) {
        const ids = nodeIdsByCallId.get(callNodeId);
        if (ids) ids.push(nodeId);
        else nodeIdsByCallId.set(callNodeId, [nodeId]);
      }
    }
  }

  if (nodeIdsByCallId.size > 0) {
    const callsByPayloadId = callIdsByPayloadId(run);
    for (const payload of run.payloads) {
      if (!isCallValuePayload(payload)) continue;
      const value = payloadToTraceCallValue(run.boundaryId, payload, valueBodyCache);

      const nodeIds = new Set<string>();
      for (const callId of callIdsForPayload(payload, callsByPayloadId)) {
        for (const nodeId of nodeIdsByCallId.get(callId) ?? []) {
          nodeIds.add(nodeId);
        }
      }
      if (nodeIds.size === 0) continue;

      for (const nodeId of nodeIds) {
        addGraphNodeValue(valuesByNodeId, nodeId, value);
      }
    }
  }

  const rootInput = rootInputToGraphValue(run, valueBodyCache);
  if (rootInput) {
    for (const nodeId of graphNodeIdsForRootValues(
      run,
      nodeIdsByCallId,
      options.rootGraphNodeId,
    )) {
      addGraphNodeValue(valuesByNodeId, nodeId, rootInput);
    }
  }

  const rootValue = rootResultToGraphValue(run, valueBodyCache);
  if (rootValue) {
    for (const nodeId of graphNodeIdsForRootValues(
      run,
      nodeIdsByCallId,
      options.rootGraphNodeId,
    )) {
      addGraphNodeValue(valuesByNodeId, nodeId, rootValue);
    }
  }

  for (const values of valuesByNodeId.values()) {
    values.sort(compareGraphNodeValues);
  }

  return valuesByNodeId;
}

export function buildExecutionProfileProjection(
  run: Run | null | undefined,
): ExecutionProfileProjection {
  if (!run || run.calls.length === 0) return emptyExecutionProfileProjection();
  const callsById = new Map(run.calls.map((call) => [call.id, call]));
  const threadParentCallIds = threadParentCallIdsByThread(run, callsById);
  const childrenByParent = new Map<string | null, Run['calls']>();
  const parentById = new Map<string, string | null>();

  for (const call of run.calls) {
    const parentId = effectiveParentId(call, callsById, threadParentCallIds);
    parentById.set(call.id, parentId);
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
  const threadLabels = threadLabelsById(run.calls);
  const blocks: ExecutionProfileBlock[] = [];

  const visit = (call: Run['calls'][number], depth: number) => {
    blocks.push(
      callToExecutionProfileBlock(
        call,
        parentById.get(call.id) ?? null,
        depth,
        childrenByParent,
        threadLabels,
        zeroNs,
        spanNs,
      ),
    );
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

  return aggregateExecutionProfileBlocks(
    blocks,
    spanNs > 0n ? Number(spanNs) / 1_000_000 : null,
  );
}

export function filterExecutionProfileProjection(
  projection: ExecutionProfileProjection,
  filters: ExecutionProfileFilters,
): ExecutionProfileProjection {
  const selected = projection.blocks.filter((block) => {
    if (!filters.includeSystemCalls && block.isSystemFrame) return false;
    return true;
  });
  if (selected.length === 0) {
    return {
      ...emptyExecutionProfileProjection(),
      totalDurationMs: projection.totalDurationMs,
    };
  }

  const visibleIds = new Set(selected.map((block) => block.id));
  const selectedById = new Map(selected.map((block) => [block.id, block]));
  const parentById = new Map(projection.blocks.map((block) => [block.id, block.parentId]));
  const visibleParentById = new Map<string, string | null>();
  for (const block of selected) {
    let parentId = block.parentId;
    const seen = new Set<string>([block.id]);
    while (parentId && !visibleIds.has(parentId) && !seen.has(parentId)) {
      seen.add(parentId);
      parentId = parentById.get(parentId) ?? null;
    }
    visibleParentById.set(
      block.id,
      parentId && parentId !== block.id && visibleIds.has(parentId) ? parentId : null,
    );
  }

  const visibleChildrenByParent = new Map<string | null, ExecutionProfileBlock[]>();
  for (const block of selected) {
    const parentId = visibleParentById.get(block.id) ?? null;
    const children = visibleChildrenByParent.get(parentId);
    if (children) {
      children.push(block);
    } else {
      visibleChildrenByParent.set(parentId, [block]);
    }
  }

  const depthById = new Map<string, number>();
  const depthOf = (block: ExecutionProfileBlock): number => {
    const cached = depthById.get(block.id);
    if (cached != null) return cached;
    const parentId = visibleParentById.get(block.id) ?? null;
    const parent = parentId ? selectedById.get(parentId) : null;
    const depth = parent ? depthOf(parent) + 1 : 0;
    depthById.set(block.id, depth);
    return depth;
  };

  const visibleBlocks = selected.map((block) => {
    const parentId = visibleParentById.get(block.id) ?? null;
    const visibleChildDurationMs = (visibleChildrenByParent.get(block.id) ?? []).reduce(
      (total, child) => total + (child.durationMs ?? 0),
      0,
    );
    return {
      ...block,
      parentId,
      depth: depthOf(block),
      selfMs:
        block.durationMs != null
          ? Math.max(0, block.durationMs - visibleChildDurationMs)
          : null,
    };
  });

  return aggregateExecutionProfileBlocks(visibleBlocks, projection.totalDurationMs);
}

export function executionProfileSearchFunctionKeys(
  projection: ExecutionProfileProjection,
  search: string,
): string[] {
  const query = search.trim().toLowerCase();
  if (query.length === 0) return [];
  return projection.functionRows
    .filter((row) => row.functionName.toLowerCase().includes(query))
    .map((row) => row.functionKey);
}

export function executionProfileColorKey(
  block: ExecutionProfileBlock,
  mode: ExecutionProfileColorMode,
): string {
  switch (mode) {
    case 'function':
      return block.functionKey;
    case 'origin':
      return block.origin;
    case 'thread':
      return block.threadId;
    default:
      mode satisfies never;
      return block.functionKey;
  }
}

function callToExecutionProfileBlock(
  call: Run['calls'][number],
  parentId: string | null,
  depth: number,
  childrenByParent: Map<string | null, Run['calls']>,
  threadLabels: Map<string, string>,
  zeroNs: bigint,
  spanNs: bigint,
): ExecutionProfileBlock {
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
  const functionName = call.functionName ?? `function#${call.functionId}`;
  const origin = executionProfileOrigin(call, functionName);
  const isSystemFrame = isExecutionProfileSystemFrame(functionName, origin);

  return {
    id: call.id,
    threadId: call.threadId,
    threadLabel: threadLabels.get(call.threadId) ?? 'T?',
    parentId,
    depth,
    functionKey: `${origin}:${functionName}`,
    functionName,
    origin,
    status: call.status,
    durationMs: durationNs !== null ? Number(durationNs) / 1_000_000 : null,
    selfMs: selfNs !== null ? Number(selfNs) / 1_000_000 : null,
    spanLeftPct: leftPct,
    spanWidthPct: Math.min(widthPct, 100 - leftPct),
    isSystemFrame,
  };
}

function aggregateExecutionProfileBlocks(
  blocks: ExecutionProfileBlock[],
  totalDurationMs: number | null,
): ExecutionProfileProjection {
  const rowsByKey = new Map<string, ExecutionProfileFunctionRow>();
  for (const block of blocks) {
    const row = rowsByKey.get(block.functionKey);
    if (row) {
      row.callCount += 1;
      row.selfMs += block.selfMs ?? 0;
      row.totalMs += block.durationMs ?? 0;
      continue;
    }
    rowsByKey.set(block.functionKey, {
      functionKey: block.functionKey,
      functionName: block.functionName,
      origin: block.origin,
      callCount: 1,
      selfMs: block.selfMs ?? 0,
      totalMs: block.durationMs ?? 0,
    });
  }
  const functionRows = [...rowsByKey.values()].sort(
    (left, right) =>
      right.totalMs - left.totalMs ||
      right.selfMs - left.selfMs ||
      left.functionName.localeCompare(right.functionName),
  );
  return {
    blocks,
    functionRows,
    maxSelfMs: functionRows.reduce((max, row) => Math.max(max, row.selfMs), 0),
    maxTotalMs: functionRows.reduce((max, row) => Math.max(max, row.totalMs), 0),
    totalDurationMs,
  };
}

function emptyExecutionProfileProjection(): ExecutionProfileProjection {
  return {
    blocks: [],
    functionRows: [],
    maxSelfMs: 0,
    maxTotalMs: 0,
    totalDurationMs: null,
  };
}

function threadLabelsById(calls: Run['calls']): Map<string, string> {
  const labels = new Map<string, string>();
  for (const call of [...calls].sort(compareCallsByStart)) {
    if (!labels.has(call.threadId)) {
      labels.set(call.threadId, `T${labels.size + 1}`);
    }
  }
  return labels;
}

function executionProfileOrigin(
  call: Run['calls'][number],
  functionName: string,
): ExecutionProfileOrigin {
  if (functionName.startsWith('baml.') || functionName.includes('<lambda')) {
    return 'system';
  }
  switch (call.functionOrigin) {
    case 'user':
      return 'user';
    case 'builtin':
    case 'companion':
    case 'internal':
      return 'system';
    case 'unknown':
    case null:
      return 'unknown';
    default:
      call.functionOrigin satisfies never;
      return 'unknown';
  }
}

function isExecutionProfileSystemFrame(
  functionName: string,
  origin: ExecutionProfileOrigin,
): boolean {
  return (
    origin === 'system' ||
    functionName.startsWith('baml.') ||
    functionName.includes('<lambda')
  );
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
  threadParentCallIds: Map<string, string>,
): number {
  let depth = 0;
  const seen = new Set<string>([call.id]);
  let parentId = effectiveParentId(call, callsById, threadParentCallIds);
  while (parentId && depth < 32 && !seen.has(parentId)) {
    const parent = callsById.get(parentId);
    if (!parent) break;
    seen.add(parentId);
    depth += 1;
    parentId = effectiveParentId(parent, callsById, threadParentCallIds);
  }
  return depth;
}

function threadParentCallIdsByThread(
  run: Run,
  callsById: Map<string, Run['calls'][number]>,
): Map<string, string> {
  const parents = new Map<string, string>();
  for (const thread of run.threads) {
    const parentId = thread.parentCallNodeId;
    if (parentId && callsById.has(parentId)) {
      parents.set(thread.id, parentId);
    }
  }
  return parents;
}

function effectiveParentId(
  call: Run['calls'][number],
  callsById: Map<string, Run['calls'][number]>,
  threadParentCallIds: Map<string, string>,
): string | null {
  if (call.parentId && callsById.has(call.parentId)) {
    return call.parentId;
  }
  const threadParentId = threadParentCallIds.get(call.threadId);
  if (threadParentId && threadParentId !== call.id && callsById.has(threadParentId)) {
    return threadParentId;
  }
  return null;
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

export function decodeRunResultValue(
  run: Run,
  valueBodyCache?: ValueBodyCache,
): BamlJsValue | null {
  const valueRef = run.result?.valueRef;
  if (valueRef) return decodeValueRef(run.boundaryId, valueRef, valueBodyCache);
  const encoded = run.result?.value;
  if (!encoded) return null;
  return decodeBase64BamlOutboundValue(encoded);
}

export function decodeRunErrorValue(
  run: Run,
  valueBodyCache?: ValueBodyCache,
): BamlJsValue | null {
  const valueRef = run.error?.valueRef;
  if (!valueRef) return null;
  return decodeValueRef(run.boundaryId, valueRef, valueBodyCache);
}

export function decodeRootInputValue(
  run: Run,
  valueBodyCache?: ValueBodyCache,
): BamlJsValue | null {
  for (const payload of run.payloads) {
    const kind = payload.kind;
    if (kind.type === 'capturedValue' && kind.role === 'rootInput') {
      return decodeValueRef(run.boundaryId, kind.valueRef, valueBodyCache);
    }
  }
  return null;
}

function traceLogsByCallId(
  run: Run,
  valueBodyCache?: ValueBodyCache,
): Map<string, RunTraceLog[]> {
  const logsByCallId = new Map<string, RunTraceLog[]>();
  const callsByPayloadId = callIdsByPayloadId(run);

  for (const payload of run.payloads) {
    if (!isLogPayload(payload)) continue;
    const callIds = callIdsForPayload(payload, callsByPayloadId);
    if (callIds.size === 0) continue;
    const log = payloadToTraceLog(run.boundaryId, payload, valueBodyCache);
    for (const callId of callIds) {
      const logs = logsByCallId.get(callId);
      if (logs) {
        logs.push(log);
      } else {
        logsByCallId.set(callId, [log]);
      }
    }
  }

  for (const logs of logsByCallId.values()) {
    logs.sort((a, b) => a.timestampMs - b.timestampMs || a.id.localeCompare(b.id));
  }
  return logsByCallId;
}

/**
 * Ordered `baml.io` stream writes for a run.
 *
 * stdout and stderr stay interleaved in emission order, the way a real
 * terminal shows them. The `stream` tag is kept for filtering, not for
 * reordering: pulling one stream out on its own would scramble the sequence.
 */
export function runToOutputChunks(run: Run): RunOutputChunk[] {
  const chunks: RunOutputChunk[] = [];
  for (const payload of run.payloads) {
    if (!isOutputPayload(payload)) continue;
    chunks.push({
      id: payload.id,
      stream: payload.kind.stream,
      text: payload.kind.text,
      timestampMs: payload.timestampMs,
    });
  }
  return chunks;
}

function traceCallValuesByCallId(
  run: Run,
  valueBodyCache?: ValueBodyCache,
): Map<string, RunTraceCallValue[]> {
  const valuesByCallId = new Map<string, RunTraceCallValue[]>();
  const callsByPayloadId = callIdsByPayloadId(run);

  for (const payload of run.payloads) {
    if (!isCallValuePayload(payload)) continue;
    const callIds = callIdsForPayload(payload, callsByPayloadId);
    if (callIds.size === 0) continue;
    const value = payloadToTraceCallValue(run.boundaryId, payload, valueBodyCache);
    for (const callId of callIds) {
      const values = valuesByCallId.get(callId);
      if (values) {
        values.push(value);
      } else {
        valuesByCallId.set(callId, [value]);
      }
    }
  }

  for (const values of valuesByCallId.values()) {
    values.sort((a, b) => a.timestampMs - b.timestampMs || a.id.localeCompare(b.id));
  }
  return valuesByCallId;
}

function callIdsByPayloadId(run: Run): Map<string, string[]> {
  const callsByPayloadId = new Map<string, string[]>();
  for (const call of run.calls) {
    for (const payloadId of call.payloadIds) {
      const ids = callsByPayloadId.get(payloadId);
      if (ids) {
        ids.push(call.id);
      } else {
        callsByPayloadId.set(payloadId, [call.id]);
      }
    }
  }
  return callsByPayloadId;
}

function callIdsForPayload(
  payload: PayloadEvent,
  callsByPayloadId: Map<string, string[]>,
): Set<string> {
  const callIds = new Set<string>();
  if (payload.callNodeId) callIds.add(payload.callNodeId);
  for (const callId of callsByPayloadId.get(payload.id) ?? []) {
    callIds.add(callId);
  }
  return callIds;
}

function payloadToTraceLog(
  boundaryId: BoundaryId,
  payload: LogPayloadEvent,
  valueBodyCache?: ValueBodyCache,
): RunTraceLog {
  const projected = payload.kind.valueRef
    ? projectValueRef(boundaryId, payload.kind.valueRef, valueBodyCache)
    : projectPayloadBodyState(payload.body);
  return {
    id: payload.id,
    timestampMs: payload.timestampMs,
    level: payload.kind.level,
    message: payload.kind.message,
    sourceLine: payload.kind.source?.line ?? null,
    valueRef: payload.kind.valueRef,
    value: projected.value,
    state: projected.state,
    diagnostic: projected.diagnostic,
  };
}

function payloadToTraceCallValue(
  boundaryId: BoundaryId,
  payload: CallValuePayloadEvent,
  valueBodyCache?: ValueBodyCache,
): RunTraceCallValue {
  const projected = payload.kind.valueRef
    ? projectValueRef(boundaryId, payload.kind.valueRef, valueBodyCache)
    : projectPayloadBodyState(payload.body);
  return {
    id: payload.id,
    timestampMs: payload.timestampMs,
    role: payload.kind.role,
    label: payload.kind.label,
    valueRef: payload.kind.valueRef,
    value: projected.value,
    state: projected.state,
    diagnostic: projected.diagnostic,
  };
}

function graphNodeIdsForRootValues(
  run: Run,
  nodeIdsByCallId: Map<string, string[]>,
  rootGraphNodeId: string | null | undefined,
): string[] {
  if (rootGraphNodeId) return [rootGraphNodeId];

  const rootCallNodeId = run.rootCallNodeId;
  if (!rootCallNodeId) return [];

  const directNodeIds = nodeIdsByCallId.get(rootCallNodeId);
  return directNodeIds ?? [];
}

function rootInputToGraphValue(
  run: Run,
  valueBodyCache?: ValueBodyCache,
): GraphNodeValuePreview | null {
  const payload = run.payloads.find(isRootInputPayload);
  if (!payload) return null;

  const projected = payload.kind.valueRef
    ? projectValueRef(run.boundaryId, payload.kind.valueRef, valueBodyCache)
    : projectPayloadBodyState(payload.body);
  return {
    id: payload.id,
    timestampMs: payload.timestampMs,
    role: 'callInput',
    label: payload.kind.label ?? 'inputs',
    valueRef: payload.kind.valueRef,
    value: projected.value,
    state: projected.state,
    diagnostic: projected.diagnostic,
  };
}

function rootResultToGraphValue(
  run: Run,
  valueBodyCache?: ValueBodyCache,
): GraphNodeValuePreview | null {
  if (run.error) {
    const projected = run.error.valueRef
      ? projectValueRef(run.boundaryId, run.error.valueRef, valueBodyCache)
      : {
          state: 'error' as const,
          value: null,
          diagnostic: run.error.message,
        };
    return {
      id: 'root-error',
      timestampMs: run.completedAtMs ?? run.startedAtMs ?? run.createdAtMs,
      role: 'callError',
      label: 'error',
      valueRef: run.error.valueRef,
      value: projected.value,
      state: projected.state,
      diagnostic: projected.diagnostic ?? run.error.message,
    };
  }

  if (!run.result) return null;
  const projected = run.result.valueRef
    ? projectValueRef(run.boundaryId, run.result.valueRef, valueBodyCache)
    : {
        state: 'available' as const,
        value: decodeRunResultValue(run, valueBodyCache),
        diagnostic: null,
      };
  if (projected.value === null && projected.state === 'available') return null;
  return {
    id: 'root-result',
    timestampMs: run.completedAtMs ?? run.startedAtMs ?? run.createdAtMs,
    role: 'callOutput',
    label: 'output',
    valueRef: run.result.valueRef,
    value: projected.value,
    state: projected.state,
    diagnostic: projected.diagnostic,
  };
}

function addGraphNodeValue(
  valuesByNodeId: Map<string, GraphNodeValuePreview[]>,
  nodeId: string,
  value: GraphNodeValuePreview,
) {
  const existing = valuesByNodeId.get(nodeId) ?? [];
  if (!existing.some((item) => item.id === value.id)) {
    existing.push(value);
  }
  if (existing.length > 0) valuesByNodeId.set(nodeId, existing);
}

function compareGraphNodeValues(
  left: GraphNodeValuePreview,
  right: GraphNodeValuePreview,
): number {
  return (
    graphValueRoleOrder(left.role) - graphValueRoleOrder(right.role) ||
    left.timestampMs - right.timestampMs ||
    left.id.localeCompare(right.id)
  );
}

function graphValueRoleOrder(role: RunTraceCallValueRole): number {
  switch (role) {
    case 'callInput':
      return 0;
    case 'callOutput':
      return 1;
    case 'callError':
      return 2;
    default:
      role satisfies never;
      return 3;
  }
}

function projectPayloadBodyState(
  body: PayloadEvent['body'],
): { state: RunTraceLogState; value: BamlJsValue | null; diagnostic: string | null } {
  const state = body?.state;
  if (!state) return { state: 'unavailable', value: null, diagnostic: null };
  switch (state.kind) {
    case 'truncated':
      return { state: 'truncated', value: null, diagnostic: null };
    case 'omittedByPolicy':
      return { state: 'omitted', value: null, diagnostic: null };
    case 'compacted':
      return { state: 'missing', value: null, diagnostic: null };
    case 'inlineBytes':
    case 'inlineJson':
    case 'retainedByRef':
      return { state: 'unavailable', value: null, diagnostic: null };
    default:
      state satisfies never;
      return { state: 'unavailable', value: null, diagnostic: null };
  }
}

function isLogPayload(payload: PayloadEvent): payload is LogPayloadEvent {
  return payload.kind.type === 'log';
}

function isOutputPayload(payload: PayloadEvent): payload is OutputPayloadEvent {
  return payload.kind.type === 'output';
}

function isCallValuePayload(payload: PayloadEvent): payload is CallValuePayloadEvent {
  const kind = payload.kind;
  return (
    kind.type === 'capturedValue' &&
    (kind.role === 'callInput' || kind.role === 'callOutput' || kind.role === 'callError')
  );
}

function isRootInputPayload(
  payload: PayloadEvent,
): payload is RootInputPayloadEvent {
  return payload.kind.type === 'capturedValue' && payload.kind.role === 'rootInput';
}

function projectValueRef(
  boundaryId: BoundaryId,
  valueRef: ValueRef | null,
  valueBodyCache?: ValueBodyCache,
): { state: RunTraceLogState; value: BamlJsValue | null; diagnostic: string | null } {
  if (!valueRef) return { state: 'unavailable', value: null, diagnostic: null };
  if (valueRef.availability !== 'available') {
    return {
      state: valueAvailabilityToTraceLogState(valueRef.availability),
      value: null,
      diagnostic: valueRef.diagnostic,
    };
  }

  const cached = valueBodyCache?.get(boundaryId, valueRef);
  if (!cached) {
    void valueBodyCache?.read(boundaryId, valueRef).catch(() => {});
    return { state: 'loading', value: null, diagnostic: null };
  }
  if (cached.availability !== 'available') {
    return {
      state: valueAvailabilityToTraceLogState(cached.availability),
      value: null,
      diagnostic: cached.diagnostic,
    };
  }
  if (cached.codec !== 'bamlOutboundValue' || !cached.bytes) {
    return {
      state: 'error',
      value: null,
      diagnostic: 'Unsupported log value body',
    };
  }
  try {
    return {
      state: 'available',
      value: decodeBamlOutboundValue(cached.bytes),
      diagnostic: null,
    };
  } catch {
    return {
      state: 'error',
      value: null,
      diagnostic: 'Failed to decode log value body',
    };
  }
}

function valueAvailabilityToTraceLogState(
  availability: ValueRef['availability'],
): RunTraceLogState {
  switch (availability) {
    case 'pending':
      return 'pending';
    case 'available':
      return 'available';
    case 'missing':
      return 'missing';
    case 'omitted':
      return 'omitted';
    case 'lost':
      return 'lost';
    default:
      availability satisfies never;
      return 'unavailable';
  }
}

function decodeValueRef(
  boundaryId: BoundaryId,
  valueRef: ValueRef | null,
  valueBodyCache?: ValueBodyCache,
): BamlJsValue | null {
  if (!valueRef || valueRef.availability !== 'available') return null;
  const cached = valueBodyCache?.get(boundaryId, valueRef);
  if (!cached) {
    void valueBodyCache?.read(boundaryId, valueRef).catch(() => {});
    return null;
  }
  if (
    cached.availability !== 'available' ||
    cached.codec !== 'bamlOutboundValue' ||
    !cached.bytes
  ) {
    return null;
  }
  try {
    return decodeBamlOutboundValue(cached.bytes);
  } catch {
    return null;
  }
}

function decodeBase64BamlOutboundValue(encoded: string): BamlJsValue | null {
  try {
    const binary = atob(encoded);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    return decodeBamlOutboundValue(bytes);
  } catch {
    return null;
  }
}

function decodeBamlOutboundValue(bytes: Uint8Array): BamlJsValue {
  return decodeCallResult(bytes, (key, handleType, typeName) => ({
    handle_key: key,
    handle_type: handleType,
    type_name: typeName,
  }));
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
  headers: Array<{ name: string; valueRedacted: boolean; value?: string | null }>,
): Record<string, string> {
  return Object.fromEntries(
    headers.map((header) => [
      header.name,
      header.value != null ? header.value : header.valueRedacted ? '<redacted>' : '',
    ]),
  );
}
