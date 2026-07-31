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

function callIdsByPayloadId(run: Run): Map<string, string[]> {
  const callsByPayloadId = new Map<string, string[]>();
  // The run store no longer serializes profile call nodes (§9.3 "one live
  // plane"); payload → call attribution falls back to payload.callNodeId.
  for (const call of run.calls ?? []) {
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
