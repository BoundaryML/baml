import type {
  Run,
  RunCursor,
  RunId,
  RunListFilter,
  RunPatch,
  RunPatchChange,
  RequestCommandOutcome,
  RunSummary,
} from './worker-protocol';
import type {
  RunStoreClient,
  RunSubscriptionHandle,
  RunSubscriptionEvent,
  StartPreviewRunRequest,
  StartTestRunRequest,
  StartRunRequest,
} from './run-store-client';

export interface ExecutionStoreSnapshot {
  runs: Run[];
  selectedRunId: RunId | null;
}

export type ExecutionStoreListener = (
  snapshot: ExecutionStoreSnapshot,
) => void;

export interface ExecutionStore {
  getSnapshot(): ExecutionStoreSnapshot;
  subscribe(listener: ExecutionStoreListener): () => void;
  startRun(request: StartRunRequest): Promise<RunId>;
  startPreviewRun(request: StartPreviewRunRequest): Promise<RunId>;
  startTestRun(request: StartTestRunRequest): Promise<RunId>;
  cancelRun(runId: RunId): Promise<RequestCommandOutcome | string>;
  respondToInput(
    runId: RunId,
    inputRequestId: string,
    value: string,
  ): Promise<RequestCommandOutcome | string>;
  respondToEnv(
    runId: RunId,
    envRequestId: string,
    value?: string,
  ): Promise<RequestCommandOutcome | string>;
  listRuns(filter?: RunListFilter): Promise<RunSummary[]>;
  snapshotRun(runId: RunId): Promise<Run>;
  followRun(runId: RunId, cursor?: RunCursor): string;
  applySnapshot(run: Run): void;
  applyPatch(patch: RunPatch): void;
  selectRun(runId: RunId | null): void;
  dispose(): void;
}

export function applyRunPatch(run: Run, patch: RunPatch): Run {
  if (run.runId !== patch.runId) {
    return cloneRun(run);
  }

  let next = cloneRun(run);
  for (const change of patch.changes) {
    next = applyRunPatchChange(next, change);
  }
  return { ...next, cursor: patch.cursor };
}

export function createExecutionStore(client: RunStoreClient): ExecutionStore {
  const runsById = new Map<RunId, Run>();
  const subscriptionsByRunId = new Map<RunId, RunSubscriptionHandle>();
  const listeners = new Set<ExecutionStoreListener>();
  let selectedRunId: RunId | null = null;
  let disposed = false;

  function snapshot(): ExecutionStoreSnapshot {
    return {
      runs: [...runsById.values()]
        .map(cloneRun)
        .sort((a, b) => b.createdAtMs - a.createdAtMs),
      selectedRunId,
    };
  }

  function notify() {
    const current = snapshot();
    for (const listener of listeners) {
      listener(current);
    }
  }

  function applySnapshot(run: Run): void {
    if (disposed) return;
    runsById.set(run.runId, cloneRun(run));
    if (selectedRunId == null) selectedRunId = run.runId;
    notify();
  }

  function applyPatch(patch: RunPatch): void {
    if (disposed) return;
    const run = runsById.get(patch.runId);
    if (!run) return;
    runsById.set(patch.runId, applyRunPatch(run, patch));
    notify();
  }

  async function consumeSubscription(handle: RunSubscriptionHandle) {
    for await (const event of handle.events) {
      if (disposed) return;
      await applySubscriptionEvent(event);
    }
  }

  async function applySubscriptionEvent(event: RunSubscriptionEvent) {
    switch (event.type) {
      case 'snapshot':
        applySnapshot(event.snapshot);
        return;
      case 'patch':
        applyPatch(event.patch);
        return;
      case 'cursorExpired': {
        const recovered = await client.snapshot(event.runId);
        applySnapshot(recovered);
        return;
      }
      default:
        event satisfies never;
    }
  }

  function followRun(runId: RunId, cursor?: RunCursor): string {
    const existing = subscriptionsByRunId.get(runId);
    if (existing) return existing.subscriptionId;

    const handle = client.subscribe(runId, cursor);
    subscriptionsByRunId.set(runId, handle);
    void consumeSubscription(handle).finally(() => {
      if (subscriptionsByRunId.get(runId) === handle) {
        subscriptionsByRunId.delete(runId);
      }
    });
    return handle.subscriptionId;
  }

  return {
    getSnapshot: snapshot,

    subscribe(listener) {
      listeners.add(listener);
      listener(snapshot());
      return () => {
        listeners.delete(listener);
      };
    },

    async startRun(request) {
      const runId = await client.startRun(request);
      const run = await client.snapshot(runId);
      applySnapshot(run);
      followRun(runId, run.cursor);
      return runId;
    },

    async startPreviewRun(request) {
      const runId = await client.startPreviewRun(request);
      const run = await client.snapshot(runId);
      applySnapshot(run);
      followRun(runId, run.cursor);
      return runId;
    },

    async startTestRun(request) {
      const runId = await client.startTestRun(request);
      const run = await client.snapshot(runId);
      applySnapshot(run);
      followRun(runId, run.cursor);
      return runId;
    },

    async cancelRun(runId) {
      return client.cancelRun(runId);
    },

    async respondToInput(runId, inputRequestId, value) {
      return client.respondToInput(runId, inputRequestId, value);
    },

    async respondToEnv(runId, envRequestId, value) {
      return client.respondToEnv(runId, envRequestId, value);
    },

    listRuns(filter) {
      return client.listRuns(filter);
    },

    async snapshotRun(runId) {
      const run = await client.snapshot(runId);
      applySnapshot(run);
      return run;
    },

    followRun,
    applySnapshot,
    applyPatch,

    selectRun(runId) {
      selectedRunId = runId;
      notify();
    },

    dispose() {
      disposed = true;
      for (const handle of subscriptionsByRunId.values()) {
        void handle.unsubscribe().catch(() => {});
      }
      subscriptionsByRunId.clear();
      listeners.clear();
      client.dispose();
    },
  };
}

function applyRunPatchChange(run: Run, change: RunPatchChange): Run {
  switch (change.type) {
    case 'upsertCallNode':
      return { ...run, calls: upsertById(run.calls, change.call) };
    case 'upsertThreadNode':
      return { ...run, threads: upsertById(run.threads, change.thread) };
    case 'upsertPayload':
      return { ...run, payloads: upsertById(run.payloads, change.payload) };
    case 'upsertDiagnostic':
      return {
        ...run,
        diagnostics: upsertDiagnostic(run.diagnostics, change.diagnostic),
      };
    case 'setRootCallNode':
      return { ...run, rootCallNodeId: change.callNodeId };
    case 'setGraphRuntimeOverlay':
      return { ...run, graphRuntimeOverlay: change.overlay };
    case 'setStatus':
      return { ...run, status: change.status };
    case 'complete':
      return applyCompletion(run, change.outcome);
    default:
      change satisfies never;
      return run;
  }
}

function applyCompletion(
  run: Run,
  outcome: Extract<RunPatchChange, { type: 'complete' }>['outcome'],
): Run {
  switch (outcome.status) {
    case 'succeeded':
      return {
        ...run,
        status: 'succeeded',
        result: outcome.result,
        error: null,
        cancellation: null,
      };
    case 'failed':
      return {
        ...run,
        status: 'failed',
        result: null,
        error: outcome.error,
        cancellation: null,
      };
    case 'cancelled':
      return {
        ...run,
        status: 'cancelled',
        result: null,
        error: null,
        cancellation: outcome.cancellation,
      };
    case 'panicked':
      return {
        ...run,
        status: 'panicked',
        result: null,
        error: outcome.error,
        cancellation: null,
      };
    default:
      outcome satisfies never;
      return run;
  }
}

function upsertById<T extends { id: string }>(items: T[], item: T): T[] {
  const index = items.findIndex((existing) => existing.id === item.id);
  if (index === -1) return [...items, item];
  return [...items.slice(0, index), item, ...items.slice(index + 1)];
}

function upsertDiagnostic(
  diagnostics: Run['diagnostics'],
  diagnostic: Run['diagnostics'][number],
): Run['diagnostics'] {
  const key = diagnosticKey(diagnostic);
  const index = diagnostics.findIndex((entry) => diagnosticKey(entry) === key);
  if (index === -1) return [...diagnostics, diagnostic];
  return [
    ...diagnostics.slice(0, index),
    diagnostic,
    ...diagnostics.slice(index + 1),
  ];
}

function diagnosticKey(diagnostic: Run['diagnostics'][number]): string {
  return [
    diagnostic.severity,
    diagnostic.code ?? '',
    diagnostic.callNodeId ?? '',
    diagnostic.payloadId ?? '',
    diagnostic.message,
  ].join('\0');
}

function cloneRun(run: Run): Run {
  return {
    ...run,
    timeAnchor: { ...run.timeAnchor },
    request: { ...run.request, target: { ...run.request.target } },
    target: { ...run.target },
    visibility: { ...run.visibility },
    result: run.result
      ? {
          ...run.result,
          supportingPayloadIds: [...run.result.supportingPayloadIds],
        }
      : null,
    error: run.error ? { ...run.error } : null,
    cancellation: run.cancellation ? { ...run.cancellation } : null,
    calls: run.calls.map((call) => ({ ...call })),
    threads: run.threads.map((thread) => ({
      ...thread,
      callNodeIds: [...thread.callNodeIds],
    })),
    payloads: run.payloads.map((payload) => ({ ...payload })),
    diagnostics: run.diagnostics.map((diagnostic) => ({ ...diagnostic })),
  };
}
