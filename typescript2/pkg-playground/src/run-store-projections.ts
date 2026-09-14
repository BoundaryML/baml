import type { BamlJsValue } from '@b/pkg-proto';
import { decodeCallResult } from '@b/pkg-proto';
import type { ValueBodyCache } from './value-body-cache';
import type {
  BoundaryId,
  FetchLogEntry,
  PayloadEvent,
  Run,
  RunTarget,
  ValueRef,
} from './worker-protocol';

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

export function runToDisplayRun(
  run: Run,
  argsJsonByBoundaryId: Record<string, string>,
  valueBodyCache?: ValueBodyCache,
): RunStoreDisplayRun | null {
  const identity = runDisplayIdentity(run);
  if (!identity) return null;
  return {
    argsJson:
      argsJsonByBoundaryId[run.boundaryId] ?? run.request.argsSummary ?? '',
    durationMs: runDurationMs(run),
    error: run.error?.message ?? null,
    errorValue: decodeRunErrorValue(run, valueBodyCache),
    fetchLogs: payloadsToFetchLogs(run.payloads),
    functionName: identity.functionName,
    id: run.boundaryId,
    inputRequests: payloadsToPendingInputs(run.payloads),
    kind: run.target.kind,
    outputChunks: runToOutputChunks(run),
    projectGeneration: run.request.projectGeneration,
    projectId: run.request.projectId,
    result: decodeRunResultValue(run, valueBodyCache),
    rootInput: decodeRootInputValue(run, valueBodyCache),
    startTime: run.startedAtMs ?? run.createdAtMs,
    status: runStatusToDisplayStatus(run.status),
    testName: identity.testName,
  };
}

/**
 * Values to show on graph nodes.
 *
 * Only the root node's input and result can be placed today: attaching a
 * value to an inner node needs a call-to-CFG-node mapping, and the overlay
 * that used to carry one died with the old profiler. Rebuilding it from
 * `profiles-v1` call sites is tracked separately; until then this attaches
 * nothing to inner nodes rather than guessing at one.
 */
export function runToGraphNodeValues(
  run: Run | null | undefined,
  valueBodyCache?: ValueBodyCache,
  options: RunToGraphNodeValuesOptions = {},
): Map<string, GraphNodeValuePreview[]> {
  const valuesByNodeId = new Map<string, GraphNodeValuePreview[]>();
  if (!run) {
    return valuesByNodeId;
  }

  const rootNodeIds = graphNodeIdsForRootValues(options.rootGraphNodeId);
  const rootInput = rootInputToGraphValue(run, valueBodyCache);
  if (rootInput) {
    for (const nodeId of rootNodeIds) {
      addGraphNodeValue(valuesByNodeId, nodeId, rootInput);
    }
  }

  const rootValue = rootResultToGraphValue(run, valueBodyCache);
  if (rootValue) {
    for (const nodeId of rootNodeIds) {
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

function payloadToTraceCallValue(
  boundaryId: BoundaryId,
  payload: CallValuePayloadEvent,
  valueBodyCache?: ValueBodyCache,
): RunTraceCallValue {
  const projected = payload.kind.valueRef
    ? projectValueRef(boundaryId, payload.kind.valueRef, valueBodyCache)
    : projectPayloadBodyState(payload.body);
  return {
    diagnostic: projected.diagnostic,
    id: payload.id,
    label: payload.kind.label,
    role: payload.kind.role,
    state: projected.state,
    timestampMs: payload.timestampMs,
    value: projected.value,
    valueRef: payload.kind.valueRef,
  };
}

function graphNodeIdsForRootValues(
  rootGraphNodeId: string | null | undefined,
): string[] {
  return rootGraphNodeId ? [rootGraphNodeId] : [];
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
    diagnostic: projected.diagnostic,
    id: payload.id,
    label: payload.kind.label ?? 'inputs',
    role: 'callInput',
    state: projected.state,
    timestampMs: payload.timestampMs,
    value: projected.value,
    valueRef: payload.kind.valueRef,
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
          diagnostic: run.error.message,
          state: 'error' as const,
          value: null,
        };
    return {
      diagnostic: projected.diagnostic ?? run.error.message,
      id: 'root-error',
      label: 'error',
      role: 'callError',
      state: projected.state,
      timestampMs: run.completedAtMs ?? run.startedAtMs ?? run.createdAtMs,
      value: projected.value,
      valueRef: run.error.valueRef,
    };
  }

  if (!run.result) return null;
  const projected = run.result.valueRef
    ? projectValueRef(run.boundaryId, run.result.valueRef, valueBodyCache)
    : {
        diagnostic: null,
        state: 'available' as const,
        value: decodeRunResultValue(run, valueBodyCache),
      };
  if (projected.value === null && projected.state === 'available') return null;
  return {
    diagnostic: projected.diagnostic,
    id: 'root-result',
    label: 'output',
    role: 'callOutput',
    state: projected.state,
    timestampMs: run.completedAtMs ?? run.startedAtMs ?? run.createdAtMs,
    value: projected.value,
    valueRef: run.result.valueRef,
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

function projectPayloadBodyState(body: PayloadEvent['body']): {
  state: RunTraceLogState;
  value: BamlJsValue | null;
  diagnostic: string | null;
} {
  const state = body?.state;
  if (!state) return { diagnostic: null, state: 'unavailable', value: null };
  switch (state.kind) {
    case 'truncated':
      return { diagnostic: null, state: 'truncated', value: null };
    case 'omittedByPolicy':
      return { diagnostic: null, state: 'omitted', value: null };
    case 'compacted':
      return { diagnostic: null, state: 'missing', value: null };
    case 'inlineBytes':
    case 'inlineJson':
    case 'retainedByRef':
      return { diagnostic: null, state: 'unavailable', value: null };
    default:
      state satisfies never;
      return { diagnostic: null, state: 'unavailable', value: null };
  }
}

function isLogPayload(payload: PayloadEvent): payload is LogPayloadEvent {
  return payload.kind.type === 'log';
}

function isOutputPayload(payload: PayloadEvent): payload is OutputPayloadEvent {
  return payload.kind.type === 'output';
}

function isCallValuePayload(
  payload: PayloadEvent,
): payload is CallValuePayloadEvent {
  const kind = payload.kind;
  return (
    kind.type === 'capturedValue' &&
    (kind.role === 'callInput' ||
      kind.role === 'callOutput' ||
      kind.role === 'callError')
  );
}

function isRootInputPayload(
  payload: PayloadEvent,
): payload is RootInputPayloadEvent {
  return (
    payload.kind.type === 'capturedValue' && payload.kind.role === 'rootInput'
  );
}

function projectValueRef(
  boundaryId: BoundaryId,
  valueRef: ValueRef | null,
  valueBodyCache?: ValueBodyCache,
): {
  state: RunTraceLogState;
  value: BamlJsValue | null;
  diagnostic: string | null;
} {
  if (!valueRef) return { diagnostic: null, state: 'unavailable', value: null };
  if (valueRef.availability !== 'available') {
    return {
      diagnostic: valueRef.diagnostic,
      state: valueAvailabilityToTraceLogState(valueRef.availability),
      value: null,
    };
  }

  const cached = valueBodyCache?.get(boundaryId, valueRef);
  if (!cached) {
    void valueBodyCache?.read(boundaryId, valueRef).catch(() => {});
    return { diagnostic: null, state: 'loading', value: null };
  }
  if (cached.availability !== 'available') {
    return {
      diagnostic: cached.diagnostic,
      state: valueAvailabilityToTraceLogState(cached.availability),
      value: null,
    };
  }
  if (cached.codec !== 'bamlOutboundValue' || !cached.bytes) {
    return {
      diagnostic: 'Unsupported log value body',
      state: 'error',
      value: null,
    };
  }
  try {
    return {
      diagnostic: null,
      state: 'available',
      value: decodeBamlOutboundValue(cached.bytes),
    };
  } catch {
    return {
      diagnostic: 'Failed to decode log value body',
      state: 'error',
      value: null,
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
        durationMs: null,
        error: null,
        id: Number(kind.fetchId),
        method: kind.method,
        requestBody: '',
        requestHeaders: headersToRecord(kind.requestHeaders),
        responseBody: null,
        responseHeaders: null,
        status: null,
        timestamp: payload.timestampMs,
        url: kind.url,
      });
      continue;
    }
    if (kind.type !== 'fetchUpdated') continue;

    const existing = logs.get(kind.fetchId);
    logs.set(kind.fetchId, {
      durationMs: kind.durationMs,
      error: kind.error,
      id: Number(kind.fetchId),
      method: existing?.method ?? '',
      requestBody: existing?.requestBody ?? '',
      requestHeaders: existing?.requestHeaders ?? {},
      responseBody: existing?.responseBody ?? null,
      responseHeaders: headersToRecord(kind.responseHeaders),
      status: kind.status,
      timestamp: existing?.timestamp ?? payload.timestampMs,
      url: existing?.url ?? '',
    });
  }
  return [...logs.values()].sort((a, b) => a.timestamp - b.timestamp);
}

function headersToRecord(
  headers: Array<{
    name: string;
    valueRedacted: boolean;
    value?: string | null;
  }>,
): Record<string, string> {
  return Object.fromEntries(
    headers.map((header) => [
      header.name,
      header.value != null
        ? header.value
        : header.valueRedacted
          ? '<redacted>'
          : '',
    ]),
  );
}
