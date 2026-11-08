// @vitest-environment jsdom

import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest';

import type { ExecutionListResult, RunStoreClient } from '../run-store-client';
import type { TelemetryExecution } from '../worker-protocol';
import {
  type TelemetryState,
  type UseTelemetryOptions,
  useTelemetry,
} from './use-telemetry';

const reactGlobal = globalThis as typeof globalThis & {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
};
const previousActEnvironment = reactGlobal.IS_REACT_ACT_ENVIRONMENT;
reactGlobal.IS_REACT_ACT_ENVIRONMENT = true;

afterAll(() => {
  if (previousActEnvironment === undefined) {
    delete reactGlobal.IS_REACT_ACT_ENVIRONMENT;
  } else {
    reactGlobal.IS_REACT_ACT_ENVIRONMENT = previousActEnvironment;
  }
});

const NOW = 1_700_000_000_000;

function execution(
  overrides: Partial<TelemetryExecution> & { executionId: string },
): TelemetryExecution {
  return {
    callsRetained: 1,
    durationNs: 1_000_000,
    entryFqn: 'user.quick',
    indexState: 'complete',
    revisionId: null,
    sourceLabel: 'playground',
    startedAtMs: NOW - 60_000,
    status: 'succeeded',
    threadsTotal: 1,
    totalCalls: 1,
    totalErrors: 0,
    valueState: 'complete',
    ...overrides,
  };
}

const quick = execution({ executionId: 'quick' });
const slowRunning = execution({
  durationNs: null,
  entryFqn: null,
  executionId: 'slow',
  startedAtMs: NOW,
  status: 'running',
});
const slowDone = execution({
  durationNs: 10_000_000_000,
  entryFqn: 'user.slow',
  executionId: 'slow',
  startedAtMs: NOW,
});
const shortDone = execution({
  durationNs: 700_000_000,
  entryFqn: 'user.short',
  executionId: 'short',
  startedAtMs: NOW,
});

/** Answers each list read with the next result, repeating the last. */
function storeClient(results: TelemetryExecution[][]) {
  let reads = 0;
  const listExecutions = vi.fn(async (): Promise<ExecutionListResult> => {
    const executions = [...results[Math.min(reads, results.length - 1)]];
    reads += 1;
    return { executions, storeMissing: false };
  });
  const client = { listExecutions } as unknown as RunStoreClient;
  return { client, listExecutions };
}

let container: HTMLDivElement;
let root: Root;
let state: TelemetryState;

function Probe(props: UseTelemetryOptions) {
  state = useTelemetry(props);
  return null;
}

async function render(props: UseTelemetryOptions) {
  await act(async () => {
    root.render(<Probe {...props} />);
  });
}

/**
 * Advances the clock in small steps: React applies the updates a timer
 * schedules only when the surrounding `act` ends, so one long step would
 * fold several polls into one.
 */
async function wait(ms: number) {
  for (let elapsed = 0; elapsed < ms; elapsed += 100) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
  }
}

const ids = () => state.executions.map((row) => `${row.id}:${row.status}`);

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  container = document.createElement('div');
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  vi.useRealTimers();
});

describe('useTelemetry while a playground run is in flight', () => {
  const llmFunctions = new Set<string>();

  it('lists a run the store had not published when the tab opened', async () => {
    // The tab opens just after the run starts: the first read predates the
    // store publishing it, so the list holds no running row to poll on.
    const { client, listExecutions } = storeClient([
      [quick],
      [slowRunning, quick],
    ]);
    const props = { active: true, client, llmFunctions, project: '/p' };
    await render({ ...props, running: true });
    expect(ids()).toEqual(['quick:succeeded']);

    await wait(2000);
    expect(listExecutions).toHaveBeenCalledTimes(2);
    expect(ids()).toEqual(['slow:running', 'quick:succeeded']);
  });

  it('lists a run that ended before the store published it', async () => {
    // A short run starts and ends between two polls. The read its end
    // triggers still comes before the store publishes it.
    const { client, listExecutions } = storeClient([
      [quick],
      [quick],
      [shortDone, quick],
    ]);
    const props = { active: true, client, llmFunctions, project: '/p' };
    await render({ ...props, revision: 0 });
    await render({ ...props, revision: 1 });
    expect(listExecutions).toHaveBeenCalledTimes(2);
    expect(ids()).toEqual(['quick:succeeded']);

    await wait(2000);
    expect(ids()).toEqual(['short:succeeded', 'quick:succeeded']);
  });

  it('stops re-reading once the run ended and is listed as ended', async () => {
    const { client, listExecutions } = storeClient([
      [quick],
      [slowDone, quick],
    ]);
    const props = { active: true, client, llmFunctions, project: '/p' };
    await render({ ...props, revision: 0, running: true });
    await render({ ...props, revision: 1, running: false });
    expect(ids()).toEqual(['slow:succeeded', 'quick:succeeded']);

    await wait(10_000);
    const reads = listExecutions.mock.calls.length;
    await wait(30_000);
    expect(listExecutions).toHaveBeenCalledTimes(reads);
  });

  it('does not poll when no run is in flight or just ended', async () => {
    const { client, listExecutions } = storeClient([[quick]]);
    await render({
      active: true,
      client,
      llmFunctions,
      project: '/p',
      revision: 3,
    });

    await wait(30_000);
    expect(listExecutions).toHaveBeenCalledTimes(1);
  });

  it('reads once when the project changes, though its run count differs', async () => {
    const { client, listExecutions } = storeClient([[quick]]);
    const props = { active: true, client, llmFunctions };
    await render({ ...props, project: '/a', revision: 3 });
    await render({ ...props, project: '/b', revision: 1 });
    expect(listExecutions).toHaveBeenCalledTimes(2);

    await wait(10_000);
    expect(listExecutions).toHaveBeenCalledTimes(2);
  });

  it('drops the previous project’s re-reads when the project changes', async () => {
    const { client, listExecutions } = storeClient([[quick]]);
    const props = { active: true, client, llmFunctions };
    await render({ ...props, project: '/a', revision: 0 });
    // A run in /a ends, then the user switches to /b straight away.
    await render({ ...props, project: '/a', revision: 1 });
    await render({ ...props, project: '/b', revision: 0 });
    expect(listExecutions).toHaveBeenCalledTimes(3);

    await wait(10_000);
    expect(listExecutions).toHaveBeenCalledTimes(3);
  });

  it('does not poll while the tab is hidden', async () => {
    const { client, listExecutions } = storeClient([[quick]]);
    await render({
      active: false,
      client,
      llmFunctions,
      project: '/p',
      running: true,
    });

    await wait(10_000);
    expect(listExecutions).not.toHaveBeenCalled();
  });
});
