import { describe, expect, it } from 'vitest';

import type {
  ExecutionTelemetry,
  TelemetryCall,
  TelemetryCallPath,
  TelemetryErrorCapture,
  TelemetryExecution,
  TelemetryThread,
} from '../worker-protocol';
import {
  buildEvidence,
  holdsMedia,
  sourceKindOf,
  toExecutionRow,
} from './evidence';

const MS = 1_000_000;

function callPath(
  overrides: Partial<TelemetryCallPath> & { callPathId: string },
): TelemetryCallPath {
  return {
    awaitNs: 0,
    callSiteEnd: null,
    callSiteFile: null,
    callSiteLine: null,
    callSiteStart: null,
    callsSelected: 0,
    callsStarted: 1,
    completedCancelled: 0,
    completedError: 0,
    completedOk: 1,
    depth: 0,
    directChildNs: 0,
    edgeKind: 'call',
    fqn: 'demo.Fn',
    inclusiveNs: 10 * MS,
    kind: 'bytecode',
    origin: 'user',
    overflowReason: null,
    parentCallPathId: null,
    selfNs: 10 * MS,
    timingComplete: true,
    ...overrides,
  };
}

function call(
  overrides: Partial<TelemetryCall> & { callId: string },
): TelemetryCall {
  return {
    args: null,
    argsCid: null,
    argsState: 'not_captured',
    callPathId: null,
    callSiteFile: null,
    callSiteLine: null,
    durationNs: 5 * MS,
    edgeKind: 'call',
    endedNs: 15 * MS,
    error: null,
    errorCid: null,
    errorId: null,
    errorState: 'not_applicable',
    fqn: 'demo.Fn',
    kind: 'bytecode',
    output: null,
    outputCid: null,
    outputState: 'not_captured',
    parentCallId: null,
    selectionReasons: [],
    startedNs: 10 * MS,
    status: 'ok',
    threadId: 'thread-root',
    ...overrides,
  };
}

function thread(
  overrides: Partial<TelemetryThread> & { threadId: string },
): TelemetryThread {
  return {
    endedNs: 100 * MS,
    endStatus: 'completed',
    kind: 'root',
    name: null,
    parentThreadId: null,
    spawnCallId: null,
    spawnFqn: null,
    spawnSiteFile: null,
    spawnSiteLine: null,
    startedNs: 0,
    ...overrides,
  };
}

function execution(
  overrides: Partial<TelemetryExecution> = {},
): TelemetryExecution {
  return {
    callsRetained: 1,
    durationNs: 100 * MS,
    entryFqn: 'demo.Root',
    executionId: 'exec-1',
    indexState: 'complete',
    revisionId: 'rev-1',
    sourceLabel: 'playground',
    startedAtMs: 1_700_000_000_000,
    status: 'succeeded',
    threadsTotal: 1,
    totalCalls: 1,
    totalErrors: 0,
    valueState: 'complete',
    ...overrides,
  };
}

function errorCapture(
  overrides: Partial<TelemetryErrorCapture> = {},
): TelemetryErrorCapture {
  return {
    errorId: 'err-1',
    kind: 'fresh',
    source: 'bytecode',
    stack: ['user.main', 'user.Render', 'user.Describe'],
    stackComplete: true,
    throwCallId: 'c1',
    throwCallPathId: 'p1',
    throwFqn: 'user.Describe',
    throwSiteFile: 'baml_src/main.baml',
    throwSiteLine: 28,
    throwThreadId: 'thread-root',
    value: null,
    valueCid: null,
    valueState: 'not_captured',
    ...overrides,
  };
}

function telemetry(
  overrides: Partial<ExecutionTelemetry> = {},
): ExecutionTelemetry {
  return {
    callPaths: [],
    calls: [],
    errors: [],
    execution: execution(),
    threads: [thread({ threadId: 'thread-root' })],
    ...overrides,
  };
}

describe('toExecutionRow', () => {
  it('shows the short function name, not the fully qualified one', () => {
    const row = toExecutionRow(
      execution({ entryFqn: 'checkout.ProcessOrder' }),
    );
    expect(row.target).toBe('ProcessOrder');
  });

  it('falls back to a trimmed id when no root span was retained', () => {
    // The normal state of a run in flight: the entry function is known only
    // once the root returns. Naming it after some other call it happened to
    // keep would point the reader at the wrong function, and the full wire
    // form is 56 characters.
    const row = toExecutionRow(
      execution({
        entryFqn: null,
        executionId:
          'baml_thread_1_ARD5wl79VUxhp7yIl-M8vQsAAAAAAAAAQAAAAAAAAAL',
      }),
    );
    expect(row.target).toBe('execution ARD5wl79VU');
  });

  it('treats an abandoned execution as failed, never as succeeded', () => {
    const row = toExecutionRow(execution({ status: 'abandoned' }));
    expect(row.status).toBe('failed');
  });

  it('reports a lossy index so counts are not read as totals', () => {
    const row = toExecutionRow(
      execution({ indexState: 'root_started_lost', status: 'failed' }),
    );
    expect(row.indexComplete).toBe(false);
    expect(row.recordsLost).toBe(true);
  });

  it('does not call a running execution damaged', () => {
    // A run in flight has `no_root_ended` because its root has not returned.
    // Reading that as loss told people their data was broken while they were
    // watching it being produced.
    const row = toExecutionRow(
      execution({ indexState: 'no_root_ended', status: 'running' }),
    );
    expect(row.indexComplete).toBe(false);
    expect(row.recordsLost).toBe(false);
  });

  it('does call an abandoned execution damaged', () => {
    // The same index state once the run is no longer going means the writer
    // died without sealing.
    const row = toExecutionRow(
      execution({ indexState: 'no_root_ended', status: 'abandoned' }),
    );
    expect(row.recordsLost).toBe(true);
  });

  it('treats a corrupt index as loss whatever the status', () => {
    expect(
      toExecutionRow(execution({ indexState: 'index_corrupt' })).recordsLost,
    ).toBe(true);
  });
});

describe('sourceKindOf', () => {
  it('classifies readable entry points', () => {
    expect(sourceKindOf('baml test -i commerce/**')).toBe('test');
    expect(sourceKindOf('playground')).toBe('playground');
    expect(sourceKindOf('baml run Foo')).toBe('cli');
    expect(sourceKindOf('python agents/research.py')).toBe('sdk');
  });

  it('reads no origin out of a source-snapshot hash', () => {
    // The engine passes the source snapshot's content hash as the source
    // label, so most real runs land here. Guessing 'sdk' from a hash would
    // put a confident, wrong glyph on nearly every row.
    expect(
      sourceKindOf(
        'd5743c9b836e34fa2c110194e915e9ad39e5ab8838fdd3c1352df5ff88f7d57e',
      ),
    ).toBe('unknown');
    expect(sourceKindOf(null)).toBe('unknown');
    expect(sourceKindOf('')).toBe('unknown');
  });
});

describe('toExecutionRow entry points', () => {
  it('shortens an opaque source identity instead of printing the hash', () => {
    const row = toExecutionRow(
      execution({
        sourceLabel:
          'd5743c9b836e34fa2c110194e915e9ad39e5ab8838fdd3c1352df5ff88f7d57e',
      }),
    );
    expect(row.entryPoint).toBe('source d5743c9b836e');
    expect(row.entryPointIsIdentity).toBe(true);
  });

  it('shows a readable label verbatim', () => {
    const row = toExecutionRow(execution({ sourceLabel: 'baml run main' }));
    expect(row.entryPoint).toBe('baml run main');
    expect(row.entryPointIsIdentity).toBe(false);
  });
});

describe('buildEvidence', () => {
  it('rebases times onto the execution clock', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [call({ callId: 'c1', startedNs: 40 * MS })],
        threads: [thread({ startedNs: 10 * MS, threadId: 'thread-root' })],
      }),
    );
    // Zero is the first thread start, so the call sits 30ms in.
    expect(evidence.spans[0].startMs).toBe(30);
    expect(evidence.threads[0].firstMs).toBe(0);
  });

  it('joins spans to contexts exactly, never by function name', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [
          callPath({ callPathId: 'p1', fqn: 'demo.Helper' }),
          callPath({ callPathId: 'p2', fqn: 'demo.Helper' }),
        ],
        calls: [call({ callId: 'c1', callPathId: 'p2', fqn: 'demo.Helper' })],
      }),
    );
    // Two contexts share a name; the span belongs to exactly the one the
    // store named, so a shared helper cannot be attributed to the wrong path.
    expect(evidence.spans[0].contextId).toBe('p2');
  });

  it('derives gaps by subtraction, covering unretained calls', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [callPath({ callPathId: 'p1', callsStarted: 41 })],
        calls: [
          call({ callId: 'c1', callPathId: 'p1' }),
          call({ callId: 'c2', callPathId: 'p1' }),
        ],
      }),
    );
    expect(evidence.gaps).toEqual([
      { calls: 39, contextId: 'p1', fn: 'Fn', id: 'gap:p1' },
    ]);
  });

  it('reports no gap when every call was retained', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [callPath({ callPathId: 'p1', callsStarted: 1 })],
        calls: [call({ callId: 'c1', callPathId: 'p1' })],
      }),
    );
    expect(evidence.gaps).toEqual([]);
  });

  it('does not count folded rows as gaps', () => {
    // A folded row already stands for calls no context kept separately.
    // Counting it again would report the same work twice.
    const evidence = buildEvidence(
      telemetry({
        callPaths: [
          callPath({
            callPathId: 'overflow:mem:call',
            callsStarted: 12,
            fqn: null,
            overflowReason: 'call_path_memory_unavailable',
          }),
        ],
      }),
    );
    expect(evidence.contexts[0].folded).toBe(true);
    expect(evidence.gaps).toEqual([]);
  });

  it('nests a span under a retained ancestor across unretained levels', () => {
    // root [0,100] encloses leaf [40,50]. The call between them ran but was
    // not selected, so it is absent from the retained set entirely -- the
    // chain cannot be walked, only recovered by containment.
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({
            callId: 'root',
            durationNs: 100 * MS,
            parentCallId: null,
            startedNs: 0,
          }),
          call({
            callId: 'leaf',
            durationNs: 10 * MS,
            parentCallId: 'unretained-middle',
            startedNs: 40 * MS,
          }),
        ],
      }),
    );
    const leaf = evidence.spans.find((span) => span.id === 'leaf');
    expect(leaf?.parentId).toBe('root');
  });

  it('picks the innermost enclosing span, not merely an enclosing one', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({ callId: 'outer', durationNs: 100 * MS, startedNs: 0 }),
          call({
            callId: 'middle',
            durationNs: 60 * MS,
            parentCallId: 'outer',
            startedNs: 20 * MS,
          }),
          call({
            callId: 'leaf',
            durationNs: 5 * MS,
            parentCallId: 'gone',
            startedNs: 40 * MS,
          }),
        ],
      }),
    );
    const leaf = evidence.spans.find((span) => span.id === 'leaf');
    expect(leaf?.parentId).toBe('middle');
  });

  it('keeps a span a root when nothing encloses it', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({ callId: 'first', durationNs: 10 * MS, startedNs: 0 }),
          call({
            callId: 'second',
            durationNs: 10 * MS,
            parentCallId: 'gone',
            startedNs: 40 * MS,
          }),
        ],
      }),
    );
    // Sequential, not nested: inventing a parent here would be worse than
    // showing two roots, because it would fabricate a call relationship.
    expect(evidence.spans.find((span) => span.id === 'second')?.parentId).toBe(
      null,
    );
  });

  it('does not nest a span under a call on another thread', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({ callId: 'root', durationNs: 100 * MS, startedNs: 0 }),
          call({
            callId: 'spawned',
            durationNs: 10 * MS,
            parentCallId: 'gone',
            startedNs: 40 * MS,
            threadId: 'spawn-1',
          }),
        ],
        threads: [
          thread({ threadId: 'thread-root' }),
          thread({
            kind: 'spawn',
            parentThreadId: 'thread-root',
            spawnCallId: 'root',
            startedNs: 30 * MS,
            threadId: 'spawn-1',
          }),
        ],
      }),
    );
    // Containment on the root's lane would have swallowed it. A spawned
    // thread attaches to the call that spawned it instead.
    const spawned = evidence.spans.find((span) => span.id === 'spawned');
    expect(spawned?.parentId).toBe('root');
  });

  it('treats a still-running span as enclosing what follows it', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({
            callId: 'root',
            durationNs: null,
            endedNs: null,
            startedNs: 0,
            status: 'ok',
          }),
          call({
            callId: 'leaf',
            durationNs: 5 * MS,
            parentCallId: 'gone',
            startedNs: 40 * MS,
          }),
        ],
      }),
    );
    expect(evidence.spans.find((span) => span.id === 'leaf')?.parentId).toBe(
      'root',
    );
  });

  it('sums self and await time without double counting', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [
          callPath({
            awaitNs: 30 * MS,
            callPathId: 'p1',
            directChildNs: 50 * MS,
            inclusiveNs: 100 * MS,
            selfNs: 20 * MS,
          }),
          callPath({
            awaitNs: 10 * MS,
            callPathId: 'p2',
            directChildNs: 0,
            inclusiveNs: 50 * MS,
            selfNs: 40 * MS,
          }),
        ],
      }),
    );
    // Self, await, and child time are disjoint parts of inclusive time, so
    // these sums attribute every nanosecond exactly once.
    expect(evidence.cpuMs).toBe(60);
    expect(evidence.awaitMs).toBe(40);
  });

  it('keeps await null when the store reported none', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [callPath({ awaitNs: null, callPathId: 'p1' })],
      }),
    );
    expect(evidence.awaitMs).toBe(null);
    expect(evidence.contexts[0].awaitMs).toBe(null);
  });

  it('keeps not-captured and lost values distinct', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({
            argsState: 'not_captured',
            callId: 'c1',
            errorState: 'lost:value_too_large',
            outputCid: 'bamlv_1_abc',
            outputState: 'available',
          }),
        ],
      }),
    );
    const values = evidence.spans[0].values;
    // Different facts with different remedies: widen the policy vs records
    // were dropped. Collapsing them into one "no value" state would tell
    // the reader to do the wrong thing.
    expect(values.args.availability).toEqual({ state: 'notCaptured' });
    expect(values.error.availability).toEqual({
      reason: 'value_too_large',
      state: 'lost',
    });
    expect(values.output.availability).toEqual({
      cid: 'bamlv_1_abc',
      state: 'available',
    });
  });

  it('marks timing incomplete when any context says so', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [
          callPath({ callPathId: 'p1' }),
          callPath({ callPathId: 'p2', timingComplete: false }),
        ],
      }),
    );
    expect(evidence.timingComplete).toBe(false);
  });

  it('flags spawned contexts, which overlap their parent in time', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [callPath({ callPathId: 'p1', edgeKind: 'spawn' })],
      }),
    );
    expect(evidence.contexts[0].spawn).toBe(true);
    expect(evidence.contexts[0].kind).toBe('spawn');
  });

  it('marks a function the project reports as an LLM call', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [callPath({ callPathId: 'p1', fqn: 'demo.FraudSignals' })],
      }),
      { llmFunctions: new Set(['FraudSignals']) },
    );
    expect(evidence.contexts[0].kind).toBe('llm');
  });

  it('names spawned threads and records their lineage', () => {
    const evidence = buildEvidence(
      telemetry({
        threads: [
          thread({ threadId: 'root' }),
          thread({
            kind: 'spawn',
            parentThreadId: 'root',
            spawnFqn: 'demo.WriteAuditLog',
            startedNs: 20 * MS,
            threadId: 'spawn-1',
          }),
        ],
      }),
    );
    const spawned = evidence.threads[1];
    expect(spawned.name).toBe('thread-1');
    expect(spawned.spawnedBy).toEqual({
      atMs: 20,
      fn: 'WriteAuditLog',
      thread: 'main',
    });
  });

  it('leaves per-lane busy and await unset rather than inventing them', () => {
    // The store attributes both per call path, not per thread. A lane draws
    // without a busy segment instead of with a made-up one.
    const evidence = buildEvidence(telemetry());
    expect(evidence.threads[0].busyMs).toBe(null);
    expect(evidence.threads[0].awaitMs).toBe(null);
  });

  it('reports value bodies as unreadable here, not as absent', () => {
    const evidence = buildEvidence(telemetry());
    expect(evidence.valuesReadable).toBe(false);
  });
});

describe('error captures', () => {
  it('shortens the throwing function and keeps the full stack', () => {
    const evidence = buildEvidence(telemetry({ errors: [errorCapture()] }));
    const error = evidence.errors[0];
    expect(error.fn).toBe('Describe');
    // The stack keeps fully qualified names: `user.` vs `openai.` is how a
    // reader tells their own frames from the runtime's.
    expect(error.stack).toEqual(['user.main', 'user.Render', 'user.Describe']);
    expect(error.stackComplete).toBe(true);
    expect(error.source_location).toEqual({
      end: null,
      file: 'baml_src/main.baml',
      line: 28,
      start: null,
    });
  });

  it('parses a captured error value', () => {
    const evidence = buildEvidence(
      telemetry({
        errors: [
          errorCapture({
            value: '{"provider":"openai","status_code":400}',
            valueState: 'available',
          }),
        ],
      }),
    );
    expect(evidence.errors[0].value).toEqual({
      provider: 'openai',
      status_code: 400,
    });
  });

  it('unwraps a provider body nested as a JSON string', () => {
    // Providers put the raw response in a string field, so a single parse
    // leaves the actual message as one escaped line.
    const evidence = buildEvidence(
      telemetry({
        errors: [
          errorCapture({
            value: JSON.stringify({
              detail: 'http 400',
              raw_body: JSON.stringify({
                error: { message: "Invalid value: 'input_image'." },
              }),
            }),
            valueState: 'available',
          }),
        ],
      }),
    );
    expect(evidence.errors[0].value).toEqual({
      detail: 'http 400',
      raw_body: { error: { message: "Invalid value: 'input_image'." } },
    });
  });

  it('keeps a non-JSON value as the text it is', () => {
    const evidence = buildEvidence(
      telemetry({
        errors: [errorCapture({ value: 'boom', valueState: 'available' })],
      }),
    );
    expect(evidence.errors[0].value).toBe('boom');
  });

  it('leaves a string field alone when it only looks like JSON', () => {
    const evidence = buildEvidence(
      telemetry({
        errors: [
          errorCapture({
            value: JSON.stringify({ detail: '{not actually json' }),
            valueState: 'available',
          }),
        ],
      }),
    );
    expect(evidence.errors[0].value).toEqual({ detail: '{not actually json' });
  });

  it('says why a value is missing rather than showing nothing', () => {
    const evidence = buildEvidence(
      telemetry({
        errors: [
          errorCapture({ value: null, valueState: 'lost:value_too_large' }),
        ],
      }),
    );
    expect(evidence.errors[0].value).toBe(null);
    expect(evidence.errors[0].valueUnavailable).toEqual({
      reason: 'value_too_large',
      state: 'lost',
    });
  });
});

describe('captured values', () => {
  it('parses a hydrated value body', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({
            args: '{"subject":"a dog running on the beach"}',
            argsState: 'available',
            callId: 'c1',
          }),
        ],
      }),
    );
    expect(evidence.spans[0].values.args.body).toEqual({
      subject: 'a dog running on the beach',
    });
  });

  it('offers a media fetch only when the value holds media', () => {
    // A 1.6MB image arrives as a descriptor, so the descriptor is what says
    // whether there are bytes worth fetching.
    const evidence = buildEvidence(
      telemetry({
        calls: [
          call({
            args: '{"subject":"a dog"}',
            argsCid: 'bamlv_1_aaa',
            argsState: 'available',
            callId: 'c1',
            output:
              '{"_data":{"$media":"image","mime":"image/png","bytes_len":1641918}}',
            outputCid: 'bamlv_1_bbb',
            outputState: 'available',
          }),
        ],
      }),
    );
    const values = evidence.spans[0].values;
    expect(values.output.mediaCid).toBe('bamlv_1_bbb');
    expect(values.args.mediaCid).toBe(null);
  });

  it('finds media nested inside an argument object', () => {
    // `Describe(art: image)` captures `{art: {_data: {$media: ...}}}`, so a
    // top-level-only check would miss every image passed as an argument.
    expect(
      holdsMedia({
        art: { _data: { $media: 'image', bytes_len: 10 } },
      }),
    ).toBe(true);
    expect(holdsMedia({ subject: 'a dog' })).toBe(false);
    expect(holdsMedia([{ _data: { $media: 'image' } }])).toBe(true);
  });

  it('keeps the availability reason when nothing hydrated', () => {
    const evidence = buildEvidence(
      telemetry({
        calls: [call({ args: null, argsState: 'not_captured', callId: 'c1' })],
      }),
    );
    expect(evidence.spans[0].values.args.body).toBe(null);
    expect(evidence.spans[0].values.args.availability).toEqual({
      state: 'notCaptured',
    });
  });
});

describe('subtree waiting', () => {
  it("rolls a transport leaf's wait up to the call that caused it", () => {
    // This is the real shape: the profiler records the wait on
    // `baml.http._send`, never on the model call above it. Reading a
    // frame's own awaitMs gives zero for every function a user wrote.
    const evidence = buildEvidence(
      telemetry({
        callPaths: [
          callPath({
            awaitNs: 0,
            callPathId: 'main',
            fqn: 'user.main',
            inclusiveNs: 52_000 * MS,
          }),
          callPath({
            awaitNs: 0,
            callPathId: 'draw',
            fqn: 'user.Draw',
            inclusiveNs: 44_000 * MS,
            parentCallPathId: 'main',
          }),
          callPath({
            awaitNs: 43_000 * MS,
            callPathId: 'send',
            fqn: 'baml.http._send',
            inclusiveNs: 43_100 * MS,
            parentCallPathId: 'draw',
          }),
        ],
      }),
    );
    const byId = new Map(evidence.contexts.map((c) => [c.id, c]));
    // Own wait is zero for both user frames...
    expect(byId.get('main')?.awaitMs).toBe(0);
    expect(byId.get('draw')?.awaitMs).toBe(0);
    // ...but the subtree total makes the waiting attributable.
    expect(byId.get('main')?.subtreeAwaitMs).toBe(43_000);
    expect(byId.get('draw')?.subtreeAwaitMs).toBe(43_000);
    expect(byId.get('send')?.subtreeAwaitMs).toBe(43_000);
  });

  it('sums waiting across sibling branches', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [
          callPath({ awaitNs: 0, callPathId: 'root', inclusiveNs: 100 * MS }),
          callPath({
            awaitNs: 30 * MS,
            callPathId: 'a',
            parentCallPathId: 'root',
          }),
          callPath({
            awaitNs: 20 * MS,
            callPathId: 'b',
            parentCallPathId: 'root',
          }),
        ],
      }),
    );
    const root = evidence.contexts.find((c) => c.id === 'root');
    expect(root?.subtreeAwaitMs).toBe(50);
  });

  it('reports no waiting when none was recorded', () => {
    const evidence = buildEvidence(
      telemetry({
        callPaths: [callPath({ awaitNs: 0, callPathId: 'p1' })],
      }),
    );
    expect(evidence.contexts[0].subtreeAwaitMs).toBe(0);
  });
});

describe('errored calls without captures', () => {
  it('counts errors per call path even when no span was retained', () => {
    // A retried HTTP request: the throw unwinds through several runtime
    // frames, the retry succeeds, and the execution succeeds. Nothing is
    // retained and no capture is written, so the call-path counts are the
    // only record that anything failed.
    const evidence = buildEvidence(
      telemetry({
        callPaths: [
          callPath({
            callPathId: 'send',
            completedError: 1,
            completedOk: 0,
            fqn: 'baml.http._send',
          }),
          callPath({
            callPathId: 'invoke',
            completedError: 1,
            completedOk: 0,
            fqn: 'openai.internal.invoke',
          }),
          callPath({ callPathId: 'ok', fqn: 'user.main' }),
        ],
        calls: [],
        errors: [],
      }),
    );
    const errored = evidence.contexts.filter((c) => c.errors > 0);
    expect(errored.map((c) => c.fn).sort()).toEqual(['_send', 'invoke']);
    // Nothing retained, nothing captured: the aggregates are all there is.
    expect(evidence.spans).toHaveLength(0);
    expect(evidence.errors).toHaveLength(0);
  });
});
