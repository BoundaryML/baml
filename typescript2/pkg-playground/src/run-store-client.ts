import type { RuntimePort } from './runtime-port';
import type {
  BoundaryId,
  ExecutionTelemetry,
  RequestCommandOutcome,
  Run,
  RunCursor,
  RunCursorExpiredReason,
  RunListFilter,
  RunPatch,
  RunSummary,
  TelemetryExecution,
  TelemetryMedia,
  ValueBodyResponse,
  ValueRef,
  WorkerOutMessage,
} from './worker-protocol';

export interface StartRunRequest {
  project: string;
  functionName: string;
  argsBytes: Uint8Array;
}

export interface StartPreviewRunRequest {
  project: string;
  parentFunctionName: string;
  helper: string;
  functionName: string;
  argsBytes: Uint8Array;
}

export interface StartTestRunRequest {
  project: string;
  generation: number;
  testName: string;
}

export interface RunCursorExpiredEvent {
  type: 'cursorExpired';
  boundaryId: BoundaryId;
  reason: RunCursorExpiredReason;
}

export type RunSubscriptionEvent =
  | { type: 'snapshot'; snapshot: Run }
  | { type: 'patch'; patch: RunPatch }
  | RunCursorExpiredEvent;

export interface RunSubscriptionHandle {
  subscriptionId: string;
  events: AsyncIterable<RunSubscriptionEvent>;
  unsubscribe(): Promise<RequestCommandOutcome | string>;
}

export interface RunStoreClient {
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
  /**
   * Executions in the project's profile store, newest first. Structure and
   * timing live there rather than in the run store.
   */
  listExecutions(project: string): Promise<ExecutionListResult>;
  /** One execution's threads, calling contexts, retained spans, and errors. */
  openExecution(
    project: string,
    executionId: string,
  ): Promise<ExecutionTelemetry>;
  /** One captured value's media bytes, by content id. */
  readTelemetryMedia(project: string, cid: string): Promise<TelemetryMedia>;
  openHistory(boundaryId: BoundaryId): Promise<Run>;
  snapshot(boundaryId: BoundaryId): Promise<Run>;
  readValue(
    boundaryId: BoundaryId,
    valueRef: ValueRef,
  ): Promise<ValueBodyResponse>;
  subscribe(boundaryId: BoundaryId, cursor?: RunCursor): RunSubscriptionHandle;
  unsubscribe(subscriptionId: string): Promise<RequestCommandOutcome | string>;
  dispose(): void;
}

/** Error raised when the runtime rejects a playground command. Carries the
 *  machine-readable server error code so the UI can special-case categories
 *  like `projectNotReady` without parsing message text. The message keeps the
 *  legacy `code: message` shape for displays that render it verbatim. */
export class RunCommandError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(`${code}: ${message}`);
    this.name = 'RunCommandError';
    this.code = code;
  }
}

/** Server rejection code for runs/previews while a rebuild is pending or the
 *  project has compile errors (fail-closed; see playground_server.rs). */
export const PROJECT_NOT_READY_ERROR_CODE = 'projectNotReady';

export function isProjectNotReadyError(error: unknown): boolean {
  return (
    error instanceof RunCommandError &&
    error.code === PROJECT_NOT_READY_ERROR_CODE
  );
}

export interface ExecutionListResult {
  executions: TelemetryExecution[];
  /** True when nothing has run under this project yet. */
  storeMissing: boolean;
}

type PendingRequest =
  | {
      kind: 'startRun';
      resolve: (boundaryId: BoundaryId) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'command';
      resolve: (outcome: RequestCommandOutcome | string) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'listRuns';
      resolve: (runs: RunSummary[]) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'historyList';
      resolve: (runs: RunSummary[]) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'snapshot';
      resolve: (run: Run) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'executionList';
      resolve: (result: ExecutionListResult) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'executionTelemetry';
      resolve: (telemetry: ExecutionTelemetry) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'telemetryMedia';
      resolve: (media: TelemetryMedia) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'valueBody';
      resolve: (body: ValueBodyResponse) => void;
      reject: (error: Error) => void;
    }
  | {
      kind: 'subscribe';
      subscriptionId: string;
      reject: (error: Error) => void;
    };

class AsyncQueue<T> implements AsyncIterable<T> {
  private values: T[] = [];
  private waiters: Array<(value: IteratorResult<T>) => void> = [];
  private closed = false;

  push(value: T): void {
    const waiter = this.waiters.shift();
    if (waiter) {
      waiter({ done: false, value });
    } else {
      this.values.push(value);
    }
  }

  close(): void {
    this.closed = true;
    for (const waiter of this.waiters.splice(0)) {
      waiter({ done: true, value: undefined });
    }
  }

  [Symbol.asyncIterator](): AsyncIterator<T> {
    return {
      next: () => {
        const value = this.values.shift();
        if (value !== undefined) {
          return Promise.resolve({ done: false, value });
        }
        if (this.closed) {
          return Promise.resolve({ done: true, value: undefined });
        }
        return new Promise<IteratorResult<T>>((resolve) => {
          this.waiters.push(resolve);
        });
      },
    };
  }
}

interface SubscriptionRecord {
  boundaryId: BoundaryId;
  queue: AsyncQueue<RunSubscriptionEvent>;
}

export function createRunStoreClient(port: RuntimePort): RunStoreClient {
  let nextRequestId = 1;
  let nextSubscriptionId = 1;
  const pending = new Map<number, PendingRequest>();
  const subscriptions = new Map<string, SubscriptionRecord>();

  function requestId(): number {
    return nextRequestId++;
  }

  function rejectPending(
    requestId: number,
    code: string,
    message: string,
  ): void {
    const waiter = pending.get(requestId);
    if (!waiter) return;
    pending.delete(requestId);
    waiter.reject(new RunCommandError(code, message));
  }

  const off = port.onMessage((msg: WorkerOutMessage) => {
    switch (msg.type) {
      case 'runStarted': {
        if (msg.requestId == null) return;
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'startRun') return;
        pending.delete(msg.requestId);
        waiter.resolve(msg.run.boundaryId);
        return;
      }
      case 'commandAck': {
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'command') return;
        pending.delete(msg.requestId);
        waiter.resolve(msg.outcome);
        return;
      }
      case 'commandError':
        rejectPending(msg.requestId, msg.code, msg.message);
        return;
      case 'runList': {
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'listRuns') return;
        pending.delete(msg.requestId);
        waiter.resolve(msg.runs);
        return;
      }
      case 'historyList': {
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'historyList') return;
        pending.delete(msg.requestId);
        waiter.resolve(msg.runs);
        return;
      }
      case 'executionList': {
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'executionList') return;
        pending.delete(msg.requestId);
        waiter.resolve({
          executions: msg.executions,
          storeMissing: msg.storeMissing ?? false,
        });
        return;
      }
      case 'executionTelemetry': {
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'executionTelemetry') return;
        pending.delete(msg.requestId);
        waiter.resolve(msg.telemetry);
        return;
      }
      case 'telemetryMedia': {
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'telemetryMedia') return;
        pending.delete(msg.requestId);
        waiter.resolve(msg.media);
        return;
      }
      case 'runSnapshot': {
        if (msg.requestId != null) {
          const waiter = pending.get(msg.requestId);
          if (waiter?.kind === 'snapshot') {
            pending.delete(msg.requestId);
            waiter.resolve(msg.snapshot);
            return;
          }
          if (waiter?.kind === 'subscribe') {
            subscriptions
              .get(waiter.subscriptionId)
              ?.queue.push({ snapshot: msg.snapshot, type: 'snapshot' });
            pending.delete(msg.requestId);
            return;
          }
        }
        for (const subscription of subscriptions.values()) {
          if (subscription.boundaryId === msg.boundaryId) {
            subscription.queue.push({
              snapshot: msg.snapshot,
              type: 'snapshot',
            });
          }
        }
        return;
      }
      case 'valueBody': {
        const waiter = pending.get(msg.requestId);
        if (!waiter || waiter.kind !== 'valueBody') return;
        pending.delete(msg.requestId);
        waiter.resolve(msg);
        return;
      }
      case 'runPatch':
        for (const subscription of subscriptions.values()) {
          if (subscription.boundaryId === msg.patch.boundaryId) {
            subscription.queue.push({ patch: msg.patch, type: 'patch' });
          }
        }
        return;
      case 'runCursorExpired': {
        const event: RunCursorExpiredEvent = {
          boundaryId: msg.boundaryId,
          reason: msg.reason,
          type: 'cursorExpired',
        };
        if (msg.requestId != null) {
          const waiter = pending.get(msg.requestId);
          if (waiter?.kind === 'subscribe') {
            pending.delete(msg.requestId);
          }
        }
        if (msg.subscriptionId) {
          subscriptions.get(msg.subscriptionId)?.queue.push(event);
        } else {
          for (const subscription of subscriptions.values()) {
            if (subscription.boundaryId === msg.boundaryId)
              subscription.queue.push(event);
          }
        }
        return;
      }
      default:
        return;
    }
  });

  function command(
    msg:
      | { type: 'cancelRun'; requestId: number; boundaryId: BoundaryId }
      | {
          type: 'respondToInput';
          requestId: number;
          boundaryId: BoundaryId;
          inputRequestId: string;
          value: string;
        }
      | {
          type: 'respondToEnv';
          requestId: number;
          boundaryId: BoundaryId;
          envRequestId: string;
          value?: string;
        }
      | { type: 'unsubscribe'; requestId: number; subscriptionId: string },
  ): Promise<RequestCommandOutcome | string> {
    return new Promise((resolve, reject) => {
      pending.set(msg.requestId, { kind: 'command', reject, resolve });
      port.postMessage(msg);
    });
  }

  const client: RunStoreClient = {
    cancelRun(boundaryId) {
      return command({ boundaryId, requestId: requestId(), type: 'cancelRun' });
    },

    dispose() {
      off();
      for (const waiter of pending.values()) {
        waiter.reject(new Error('RunStoreClient disposed'));
      }
      pending.clear();
      for (const subscription of subscriptions.values()) {
        subscription.queue.close();
      }
      subscriptions.clear();
    },

    listExecutions(project) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'executionList', reject, resolve });
        port.postMessage({ project, requestId: id, type: 'listExecutions' });
      });
    },

    listHistory(filter) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'historyList', reject, resolve });
        port.postMessage({ filter, requestId: id, type: 'listHistory' });
      });
    },

    listRuns(filter) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'listRuns', reject, resolve });
        port.postMessage({ filter, requestId: id, type: 'listRuns' });
      });
    },

    openExecution(project, executionId) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'executionTelemetry', reject, resolve });
        port.postMessage({
          executionId,
          project,
          requestId: id,
          type: 'openExecution',
        });
      });
    },

    openHistory(boundaryId) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'snapshot', reject, resolve });
        port.postMessage({ boundaryId, requestId: id, type: 'openHistory' });
      });
    },

    readTelemetryMedia(project, cid) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'telemetryMedia', reject, resolve });
        port.postMessage({
          cid,
          project,
          requestId: id,
          type: 'readTelemetryMedia',
        });
      });
    },

    readValue(boundaryId, valueRef) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'valueBody', reject, resolve });
        port.postMessage({
          boundaryId,
          requestId: id,
          type: 'readValue',
          valueRef,
        });
      });
    },

    respondToEnv(boundaryId, envRequestId, value) {
      return command({
        boundaryId,
        envRequestId,
        requestId: requestId(),
        type: 'respondToEnv',
        value,
      });
    },

    respondToInput(boundaryId, inputRequestId, value) {
      return command({
        boundaryId,
        inputRequestId,
        requestId: requestId(),
        type: 'respondToInput',
        value,
      });
    },

    snapshot(boundaryId) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'snapshot', reject, resolve });
        port.postMessage({ boundaryId, requestId: id, type: 'snapshot' });
      });
    },

    startPreviewRun(request) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'startRun', reject, resolve });
        port.postMessage({
          argsBytes: request.argsBytes,
          functionName: request.functionName,
          helper: request.helper,
          parentFunctionName: request.parentFunctionName,
          project: request.project,
          requestId: id,
          type: 'startPreviewRun',
        });
      });
    },
    startRun(request) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'startRun', reject, resolve });
        port.postMessage({
          argsBytes: request.argsBytes,
          functionName: request.functionName,
          project: request.project,
          requestId: id,
          type: 'startRun',
        });
      });
    },

    startTestRun(request) {
      const id = requestId();
      return new Promise((resolve, reject) => {
        pending.set(id, { kind: 'startRun', reject, resolve });
        port.postMessage({
          generation: request.generation,
          project: request.project,
          requestId: id,
          testName: request.testName,
          type: 'startTestRun',
        });
      });
    },

    subscribe(boundaryId, cursor) {
      const id = requestId();
      const subscriptionId = `run_sub_${nextSubscriptionId++}`;
      const queue = new AsyncQueue<RunSubscriptionEvent>();
      subscriptions.set(subscriptionId, { boundaryId, queue });
      pending.set(id, {
        kind: 'subscribe',
        reject: () =>
          queue.push({
            boundaryId,
            reason: 'unavailable',
            type: 'cursorExpired',
          }),
        subscriptionId,
      });
      port.postMessage({
        afterCursor: cursor,
        boundaryId,
        requestId: id,
        subscriptionId,
        type: 'subscribe',
      });
      return {
        events: queue,
        subscriptionId,
        unsubscribe: () => client.unsubscribe(subscriptionId),
      };
    },

    unsubscribe(subscriptionId) {
      subscriptions.get(subscriptionId)?.queue.close();
      subscriptions.delete(subscriptionId);
      return command({
        requestId: requestId(),
        subscriptionId,
        type: 'unsubscribe',
      });
    },
  };

  return client;
}
