import type {
  RunStoreClient,
  RunSubscriptionEvent,
  RunSubscriptionHandle,
  StartPreviewRunRequest,
  StartRunRequest,
  StartTestRunRequest,
} from './run-store-client';
import type {
  BoundaryId,
  RequestCommandOutcome,
  Run,
  RunCursor,
  RunListFilter,
  RunPatch,
  RunPatchChange,
  RunSummary,
} from './worker-protocol';

export interface ExecutionStoreSnapshot {
  runs: Run[];
  selectedBoundaryId: BoundaryId | null;
}

export type ExecutionStoreListener = (snapshot: ExecutionStoreSnapshot) => void;

export interface ExecutionStore {
  getSnapshot(): ExecutionStoreSnapshot;
  subscribe(listener: ExecutionStoreListener): () => void;
  startRun(request: StartRunRequest): Promise<BoundaryId>;
  startPreviewRun(request: StartPreviewRunRequest): Promise<BoundaryId>;
  startTestRun(request: StartTestRunRequest): Promise<BoundaryId>;
  cancelRun(boundaryId: BoundaryId): Promise<RequestCommandOutcome | string>;
  respondToInput(
    boundaryId: BoundaryId,
    inputRequestId: string,
    value: string,
  ): Promise<RequestCommandOutcome | string>;
  respondToEnv(
    boundaryId: BoundaryId,
    envRequestId: string,
    value?: string,
  ): Promise<RequestCommandOutcome | string>;
  listRuns(filter?: RunListFilter): Promise<RunSummary[]>;
  listHistory(filter?: RunListFilter): Promise<RunSummary[]>;
  openHistory(boundaryId: BoundaryId): Promise<Run>;
  snapshotRun(boundaryId: BoundaryId): Promise<Run>;
  followRun(boundaryId: BoundaryId, cursor?: RunCursor): string;
  applySnapshot(run: Run): void;
  applyPatch(patch: RunPatch): void;
  selectRun(boundaryId: BoundaryId | null): void;
  dispose(): void;
}

export function applyRunPatch(run: Run, patch: RunPatch): Run {
  if (run.boundaryId !== patch.boundaryId) {
    return cloneRun(run);
  }

  let next = cloneRun(run);
  for (const change of patch.changes) {
    next = applyRunPatchChange(next, change);
  }
  return { ...next, cursor: patch.cursor };
}

export function createExecutionStore(client: RunStoreClient): ExecutionStore {
  const runsById = new Map<BoundaryId, Run>();
  const subscriptionsByBoundaryId = new Map<
    BoundaryId,
    RunSubscriptionHandle
  >();
  const listeners = new Set<ExecutionStoreListener>();
  let selectedBoundaryId: BoundaryId | null = null;
  let disposed = false;

  function snapshot(): ExecutionStoreSnapshot {
    return {
      runs: [...runsById.values()]
        .map(cloneRun)
        .sort((a, b) => b.createdAtMs - a.createdAtMs),
      selectedBoundaryId,
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
    runsById.set(run.boundaryId, cloneRun(run));
    if (selectedBoundaryId == null) selectedBoundaryId = run.boundaryId;
    notify();
  }

  function applyPatch(patch: RunPatch): void {
    if (disposed) return;
    const run = runsById.get(patch.boundaryId);
    if (!run) return;
    runsById.set(patch.boundaryId, applyRunPatch(run, patch));
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
        const recovered = await client.snapshot(event.boundaryId);
        applySnapshot(recovered);
        return;
      }
      default:
        event satisfies never;
    }
  }

  function followRun(boundaryId: BoundaryId, cursor?: RunCursor): string {
    const existing = subscriptionsByBoundaryId.get(boundaryId);
    if (existing) return existing.subscriptionId;

    const handle = client.subscribe(boundaryId, cursor);
    subscriptionsByBoundaryId.set(boundaryId, handle);
    void consumeSubscription(handle).finally(() => {
      if (subscriptionsByBoundaryId.get(boundaryId) === handle) {
        subscriptionsByBoundaryId.delete(boundaryId);
      }
    });
    return handle.subscriptionId;
  }

  return {
    applyPatch,
    applySnapshot,

    async cancelRun(boundaryId) {
      return client.cancelRun(boundaryId);
    },

    dispose() {
      disposed = true;
      for (const handle of subscriptionsByBoundaryId.values()) {
        void handle.unsubscribe().catch(() => {});
      }
      subscriptionsByBoundaryId.clear();
      listeners.clear();
      client.dispose();
    },

    followRun,
    getSnapshot: snapshot,

    listHistory(filter) {
      return client.listHistory(filter);
    },

    listRuns(filter) {
      return client.listRuns(filter);
    },

    async openHistory(boundaryId) {
      const run = await client.openHistory(boundaryId);
      applySnapshot(run);
      return run;
    },

    async respondToEnv(boundaryId, envRequestId, value) {
      return client.respondToEnv(boundaryId, envRequestId, value);
    },

    async respondToInput(boundaryId, inputRequestId, value) {
      return client.respondToInput(boundaryId, inputRequestId, value);
    },

    selectRun(boundaryId) {
      selectedBoundaryId = boundaryId;
      notify();
    },

    async snapshotRun(boundaryId) {
      const run = await client.snapshot(boundaryId);
      applySnapshot(run);
      return run;
    },

    async startPreviewRun(request) {
      const boundaryId = await client.startPreviewRun(request);
      const run = await client.snapshot(boundaryId);
      applySnapshot(run);
      followRun(boundaryId, run.cursor);
      return boundaryId;
    },

    async startRun(request) {
      const boundaryId = await client.startRun(request);
      const run = await client.snapshot(boundaryId);
      applySnapshot(run);
      followRun(boundaryId, run.cursor);
      return boundaryId;
    },

    async startTestRun(request) {
      const boundaryId = await client.startTestRun(request);
      const run = await client.snapshot(boundaryId);
      applySnapshot(run);
      followRun(boundaryId, run.cursor);
      return boundaryId;
    },

    subscribe(listener) {
      listeners.add(listener);
      listener(snapshot());
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

function applyRunPatchChange(run: Run, change: RunPatchChange): Run {
  switch (change.type) {
    case 'upsertPayload':
      return { ...run, payloads: upsertById(run.payloads, change.payload) };
    case 'upsertDiagnostic':
      return {
        ...run,
        diagnostics: upsertDiagnostic(run.diagnostics, change.diagnostic),
      };
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
        cancellation: null,
        error: null,
        result: outcome.result,
        status: 'succeeded',
      };
    case 'failed':
      return {
        ...run,
        cancellation: null,
        error: outcome.error,
        result: null,
        status: 'failed',
      };
    case 'cancelled':
      return {
        ...run,
        cancellation: outcome.cancellation,
        error: null,
        result: null,
        status: 'cancelled',
      };
    case 'panicked':
      return {
        ...run,
        cancellation: null,
        error: outcome.error,
        result: null,
        status: 'panicked',
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
    cancellation: run.cancellation ? { ...run.cancellation } : null,
    diagnostics: run.diagnostics.map((diagnostic) => ({ ...diagnostic })),
    error: run.error ? { ...run.error } : null,
    payloads: run.payloads.map((payload) => ({ ...payload })),
    request: { ...run.request, target: { ...run.request.target } },
    result: run.result
      ? {
          ...run.result,
          supportingPayloadIds: [...run.result.supportingPayloadIds],
        }
      : null,
    target: { ...run.target },
    timeAnchor: { ...run.timeAnchor },
    visibility: { ...run.visibility },
  };
}
