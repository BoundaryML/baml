import type { BamlJsValue } from '@b/pkg-proto';
import { applyRunPatch } from './execution-store';
import { decodeRunResultValue } from './run-store-projections';
import type { RunStoreClient, RunSubscriptionEvent } from './run-store-client';
import type { ValueBodyCache } from './value-body-cache';
import type { BoundaryId, Run } from './worker-protocol';

const TERMINAL_RUN_STATUS = new Set<Run['status']>([
  'succeeded',
  'failed',
  'cancelled',
  'panicked',
]);

export interface AwaitedRun {
  /** The terminal run snapshot, or null if it never reached a terminal status. */
  run: Run | null;
  status: Run['status'] | undefined;
  /** Decoded result value (its `valueRef` body is fetched into the cache first),
   *  or null when there's no value or it couldn't be decoded. */
  value: BamlJsValue | null;
}

/**
 * Subscribe to a run, apply its snapshot + patch stream until it reaches a
 * terminal status (racing a timeout), then fetch the result `valueRef` body and
 * decode it.
 *
 * This is the canonical "await a run to completion and read its result" flow.
 * Both the playground's `ExecutionPanel` and the inline `BamlEditor` cells run
 * over the same RunStore protocol, so the terminal-detection + valueRef-fetch +
 * decode lives here rather than being reimplemented per caller — the places
 * protocol changes (runId→boundaryId, inline-value→valueRef) silently broke a
 * private copy of this loop are exactly what this prevents.
 */
export async function awaitRunCompletion(
  client: RunStoreClient,
  valueBodyCache: ValueBodyCache,
  boundaryId: BoundaryId,
  opts: { timeoutMs?: number } = {},
): Promise<AwaitedRun> {
  const timeoutMs = opts.timeoutMs ?? 30_000;
  const handle = client.subscribe(boundaryId);
  const iterator = handle.events[Symbol.asyncIterator]();
  let timeoutId: ReturnType<typeof setTimeout>;
  const timeout = new Promise<'timeout'>((resolve) => {
    timeoutId = setTimeout(() => resolve('timeout'), timeoutMs);
  });

  let run: Run | null = null;
  try {
    while (true) {
      // Attach a catch to `iterator.next()` so a transport error that settles
      // after the timeout has already won the race doesn't become an unhandled
      // rejection; treat it as end-of-stream.
      const next = await Promise.race([
        iterator.next().catch((err) => {
          console.error('[awaitRunCompletion] subscription error:', err);
          return { done: true, value: undefined } as const;
        }),
        timeout,
      ]);
      if (next === 'timeout' || next.done) break;
      const event = next.value as RunSubscriptionEvent;
      if (event.type === 'snapshot') run = event.snapshot;
      else if (event.type === 'patch' && run)
        run = applyRunPatch(run, event.patch);
      else if (event.type === 'cursorExpired')
        run = await client.snapshot(boundaryId);
      if (run && TERMINAL_RUN_STATUS.has(run.status)) break;
    }
  } finally {
    clearTimeout(timeoutId!);
    void handle.unsubscribe();
  }

  if (!run || !TERMINAL_RUN_STATUS.has(run.status)) {
    return { run, status: run?.status, value: null };
  }

  // Results come back as a `valueRef` into the runtime's value store; fetch the
  // body into the cache, then decode.
  const resultRef = run.result?.valueRef;
  if (resultRef) {
    try {
      await valueBodyCache.read(run.boundaryId, resultRef);
    } catch {
      /* fall through — decode returns null and the caller reports a failure */
    }
  }
  const value = decodeRunResultValue(run, valueBodyCache);
  return { run, status: run.status, value };
}
