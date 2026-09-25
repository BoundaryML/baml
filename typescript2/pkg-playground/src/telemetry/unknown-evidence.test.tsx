import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type {
  ExecutionTelemetry,
  TelemetryCall,
  TelemetryCallPath,
  TelemetryErrorCapture,
  TelemetryExecution,
} from '../worker-protocol';
import { buildEvidence, sumOf, toExecutionRow } from './evidence';
import { summarizeDurations } from './format';
import { ContextInspector, OverviewTab } from './TelemetryView';

const MS = 1_000_000;

/**
 * A calling context from an older BTEL recording: completed calls, but no
 * starts, selections or population outcomes.
 */
function btelPath(
  overrides: Partial<TelemetryCallPath> & { callPathId: string },
): TelemetryCallPath {
  return {
    awaitNs: 0,
    callSiteEnd: null,
    callSiteFile: null,
    callSiteLine: null,
    callSiteStart: null,
    callsSelected: null,
    callsStarted: null,
    completedCalls: 3,
    completedCancelled: null,
    completedError: null,
    completedOk: null,
    depth: 0,
    directChildNs: 0,
    edgeKind: 'root',
    fqn: 'user.main',
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

function btelExecution(
  overrides: Partial<TelemetryExecution> = {},
): TelemetryExecution {
  return {
    callsRetained: 1,
    durationNs: 20 * MS,
    entryFqn: 'user.main',
    executionId: 'rec:2',
    indexState: 'complete',
    revisionId: null,
    sourceLabel: null,
    startedAtMs: 1_700_000_000_000,
    status: 'failed',
    threadsTotal: 1,
    totalCalls: 4,
    // Older aggregates record no outcomes: how many calls errored is unknown.
    totalErrors: null,
    valueState: null,
    ...overrides,
  };
}

function erroredCall(): TelemetryCall {
  return {
    args: null,
    argsCid: null,
    argsState: 'not_captured',
    callId: 'rec:3',
    callPathId: 'rec:p2',
    callSiteFile: null,
    callSiteLine: null,
    durationNs: 5 * MS,
    edgeKind: 'call',
    endedNs: 7 * MS,
    error: '{"detail":"http 500"}',
    errorCid: 'abc',
    errorId: 'rec:3',
    errorState: 'available',
    fqn: 'user.Classify',
    kind: 'bytecode',
    output: null,
    outputCid: null,
    outputState: 'not_applicable',
    parentCallId: null,
    selectionReasons: [],
    startedNs: 2 * MS,
    status: 'errored',
    threadId: 'rec:2',
  };
}

/** What the LSP sends for a retained errored call: no throw provenance. */
function erroredCallEntry(): TelemetryErrorCapture {
  return {
    callFqn: 'user.Classify',
    callId: 'rec:3',
    callThreadId: 'rec:2',
    errorId: 'rec:3',
    grain: 'errored_call',
    kind: null,
    source: null,
    stack: [],
    stackComplete: false,
    throwCallId: null,
    throwCallPathId: null,
    throwFqn: null,
    throwSiteFile: null,
    throwSiteLine: null,
    throwThreadId: null,
    value: '{"detail":"http 500"}',
    valueCid: 'abc',
    valueState: 'available',
  };
}

function btelTelemetry(
  overrides: Partial<ExecutionTelemetry> = {},
): ExecutionTelemetry {
  return {
    callPaths: [
      btelPath({ callPathId: 'rec:p1', completedCalls: 1 }),
      btelPath({
        callPathId: 'rec:p2',
        completedCalls: 3,
        depth: 1,
        edgeKind: 'call',
        fqn: 'user.Classify',
        inclusiveNs: 6 * MS,
        parentCallPathId: 'rec:p1',
        selfNs: 6 * MS,
      }),
    ],
    calls: [erroredCall()],
    errors: [erroredCallEntry()],
    execution: btelExecution(),
    threads: [
      {
        endedNs: 20 * MS,
        endStatus: 'errored',
        kind: 'root',
        name: null,
        parentThreadId: null,
        spawnCallId: null,
        spawnFqn: null,
        spawnSiteFile: null,
        spawnSiteLine: null,
        startedNs: 0,
        threadId: 'rec:2',
      },
    ],
    ...overrides,
  };
}

const noop = () => {};

describe('unknown counts stay unknown', () => {
  it('keeps unrecorded outcomes and starts null, and uses completed calls', () => {
    const evidence = buildEvidence(btelTelemetry());
    const classify = evidence.contexts.find((c) => c.id === 'rec:p2');
    expect(classify?.errors).toBe(null);
    expect(classify?.enters).toBe(3);
    expect(evidence.totalCalls).toBe(4);
    // Gaps are subtractions from known counts: main's one call was not
    // retained, and one of Classify's three was.
    expect(evidence.gaps.map((gap) => [gap.contextId, gap.calls])).toEqual([
      ['rec:p1', 1],
      ['rec:p2', 2],
    ]);
  });

  it('never presents a partial total as the whole', () => {
    const evidence = buildEvidence(
      btelTelemetry({
        callPaths: [
          btelPath({ callPathId: 'a', selfNs: 4 * MS }),
          btelPath({ callPathId: 'b', completedCalls: null, selfNs: null }),
        ],
      }),
    );
    expect(evidence.cpuMs).toBe(null);
    expect(evidence.totalCalls).toBe(null);
    const b = evidence.contexts.find((c) => c.id === 'b');
    expect(b?.selfMs).toBe(null);
    expect(b?.enters).toBe(null);
    // No count, no subtraction: no gap is invented.
    expect(evidence.gaps.find((gap) => gap.contextId === 'b')).toBeUndefined();
    expect(sumOf([1, null, 2], (value) => value)).toBe(null);
    expect(sumOf([1, 2], (value) => value)).toBe(3);
  });

  it('rolls an unknown wait up as unknown', () => {
    const evidence = buildEvidence(
      btelTelemetry({
        callPaths: [
          btelPath({ awaitNs: 1 * MS, callPathId: 'root' }),
          btelPath({
            awaitNs: null,
            callPathId: 'child',
            parentCallPathId: 'root',
          }),
        ],
      }),
    );
    const root = evidence.contexts.find((c) => c.id === 'root');
    expect(root?.subtreeAwaitMs).toBe(null);
  });

  it('never calls retained calls a population when the count is unknown', () => {
    const evidence = buildEvidence(btelTelemetry());
    const summary = summarizeDurations(evidence.spans, null);
    expect(summary.kind).toBe('sample');
    if (summary.kind === 'sample') expect(summary.total).toBe(null);
  });

  it('shows "not recorded" rather than zero errors in the inspector', () => {
    const evidence = buildEvidence(btelTelemetry());
    const context = evidence.contexts.find((c) => c.id === 'rec:p2') ?? null;
    const markup = renderToStaticMarkup(
      <ContextInspector
        context={context}
        evidence={evidence}
        openSpan={noop}
      />,
    );
    expect(markup).toContain('Errored calls');
    expect(markup).toContain('not recorded');
    expect(markup).not.toMatch(/Errored calls<\/div><div[^>]*>0</);
  });

  it('still shows known zero errors for backends that count outcomes', () => {
    const evidence = buildEvidence(
      btelTelemetry({
        callPaths: [
          btelPath({
            callPathId: 'known',
            callsStarted: 5,
            completedCalls: undefined,
            completedCancelled: 0,
            completedError: 0,
            completedOk: 5,
          }),
        ],
        errors: [],
        execution: btelExecution({ status: 'succeeded', totalErrors: 0 }),
      }),
    );
    const context = evidence.contexts[0];
    expect(context.errors).toBe(0);
    expect(context.enters).toBe(5);
    const markup = renderToStaticMarkup(
      <ContextInspector
        context={context}
        evidence={evidence}
        openSpan={noop}
      />,
    );
    expect(markup).not.toContain('not recorded');
  });
});

describe('recorded population outcomes', () => {
  it('shows success, failure and cancellation without retained calls', () => {
    const evidence = buildEvidence(
      btelTelemetry({
        callPaths: [
          btelPath({
            callPathId: 'known',
            completedCalls: 4,
            completedCancelled: 1,
            completedError: 2,
            completedOk: 1,
            outcomeState: 'recorded',
          }),
        ],
        calls: [],
        errors: [],
      }),
    );
    const context = evidence.contexts[0];
    expect([context.ok, context.errors, context.cancelled]).toEqual([1, 2, 1]);
    const inspector = renderToStaticMarkup(
      <ContextInspector
        context={context}
        evidence={evidence}
        openSpan={noop}
      />,
    );
    expect(inspector).toMatch(/Succeeded<\/div><div[^>]*>1</);
    expect(inspector).toMatch(/Errored calls<\/div><div[^>]*>2</);
    expect(inspector).toMatch(/Cancelled<\/div><div[^>]*>1</);
    const overview = renderToStaticMarkup(
      <OverviewTab
        evidence={evidence}
        execution={toExecutionRow(btelExecution())}
        openContext={noop}
        openSpan={noop}
      />,
    );
    expect(overview).toMatch(/Errored calls<\/div><div[^>]*>2</);
    expect(overview).not.toContain('Distinct errors captured');
    expect(overview).not.toContain('0 retained');
  });

  it('explains mixed old and new evidence instead of showing partial counts', () => {
    const evidence = buildEvidence(
      btelTelemetry({
        callPaths: [btelPath({ callPathId: 'mixed', outcomeState: 'partial' })],
      }),
    );
    const markup = renderToStaticMarkup(
      <ContextInspector
        context={evidence.contexts[0]}
        evidence={evidence}
        openSpan={noop}
      />,
    );
    expect(markup).toContain('Some completed calls lack outcome evidence');
    expect(markup).not.toMatch(/Errored calls<\/div><div[^>]*>0</);
  });

  it('does not roll a truncated context population into a run total', () => {
    const evidence = buildEvidence(
      btelTelemetry({
        callPaths: [
          btelPath({
            callPathId: 'subset',
            completedCalls: 1,
            completedCancelled: 0,
            completedError: 0,
            completedOk: 1,
            outcomeState: 'recorded',
          }),
        ],
        calls: [],
        errors: [],
      }),
    );
    const overview = renderToStaticMarkup(
      <OverviewTab
        evidence={evidence}
        execution={toExecutionRow(btelExecution({ totalCalls: 4 }))}
        openContext={noop}
        openSpan={noop}
      />,
    );
    expect(overview).toContain('0 retained');
    expect(overview).not.toContain('No call errored.');
  });
});

describe('errored calls are not throws', () => {
  it('maps errored-call entries without throw provenance', () => {
    const evidence = buildEvidence(btelTelemetry());
    const [entry] = evidence.errors;
    expect(entry.grain).toBe('erroredCall');
    // The function and call the error was seen on, for pivoting into Trace.
    expect(entry.fn).toBe('Classify');
    expect(entry.callId).toBe('rec:3');
    expect(entry.stack).toEqual([]);
    expect(entry.source_location).toBe(null);
    expect(entry.value).toEqual({ detail: 'http 500' });
  });

  it('keeps equal error values on different calls as separate rows', () => {
    const second = {
      ...erroredCallEntry(),
      callId: 'rec:9',
      errorId: 'rec:9',
    };
    const evidence = buildEvidence(
      btelTelemetry({ errors: [erroredCallEntry(), second] }),
    );
    expect(evidence.errors.map((error) => error.id)).toEqual([
      'rec:3',
      'rec:9',
    ]);
  });

  it('labels them as errored calls and never claims distinct throws', () => {
    const evidence = buildEvidence(btelTelemetry());
    const execution = toExecutionRow(btelExecution());
    const markup = renderToStaticMarkup(
      <OverviewTab
        evidence={evidence}
        execution={execution}
        openContext={noop}
        openSpan={noop}
      />,
    );
    expect(markup).toContain('1 retained call ended in an error.');
    expect(markup).toContain('Where an error was thrown is not recorded');
    expect(markup).toContain('errored call');
    expect(markup).toContain('Errored calls');
    expect(markup).toContain('1 retained');
    expect(markup).not.toContain('Stack, root to throw');
    expect(markup).not.toContain('Distinct errors captured');
    expect(markup).not.toContain('1 error<');
  });

  it('does not say no call errored when outcomes are not recorded', () => {
    const evidence = buildEvidence(btelTelemetry({ calls: [], errors: [] }));
    const execution = toExecutionRow(
      btelExecution({ callsRetained: 0, status: 'succeeded' }),
    );
    const markup = renderToStaticMarkup(
      <OverviewTab
        evidence={evidence}
        execution={execution}
        openContext={noop}
        openSpan={noop}
      />,
    );
    expect(markup).not.toContain('No call errored.');
    expect(markup).toContain('No retained call errored.');
    expect(markup).toContain('0 retained');
  });

  it('keeps the old throw panel for real throw captures', () => {
    const throwCapture: TelemetryErrorCapture = {
      errorId: 'err-1',
      kind: 'fresh',
      source: 'bytecode',
      stack: ['user.main', 'user.Describe'],
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
    };
    const evidence = buildEvidence(
      btelTelemetry({
        calls: [],
        errors: [throwCapture],
        execution: btelExecution({ totalErrors: 3 }),
      }),
    );
    expect(evidence.errors[0].grain).toBe('throw');
    const markup = renderToStaticMarkup(
      <OverviewTab
        evidence={evidence}
        execution={toExecutionRow(btelExecution({ totalErrors: 3 }))}
        openContext={noop}
        openSpan={noop}
      />,
    );
    expect(markup).toContain('Stack, root to throw');
    expect(markup).toContain('1 error');
    expect(markup).not.toContain('errored call<');
  });
});

describe('no end recorded is not running', () => {
  it('keeps incomplete distinct from running for executions and calls', () => {
    const row = toExecutionRow(
      btelExecution({ indexState: 'no_root_ended', status: 'incomplete' }),
    );
    expect(row.status).toBe('incomplete');
    expect(row.recordsLost).toBe(false);
    expect(row.indexComplete).toBe(false);
    const evidence = buildEvidence(
      btelTelemetry({
        calls: [
          {
            ...erroredCall(),
            durationNs: null,
            endedNs: null,
            error: null,
            errorState: 'lost:pending',
            status: 'incomplete',
          },
        ],
        errors: [],
      }),
    );
    expect(evidence.spans[0].status).toBe('incomplete');
  });

  it('says evidence may still grow rather than that records were lost', () => {
    const execution = btelExecution({
      indexState: 'no_root_ended',
      status: 'incomplete',
    });
    const evidence = buildEvidence(btelTelemetry({ errors: [], execution }));
    const markup = renderToStaticMarkup(
      <OverviewTab
        evidence={evidence}
        execution={toExecutionRow(execution)}
        openContext={noop}
        openSpan={noop}
      />,
    );
    expect(markup).toContain('No end is recorded for this execution');
    expect(markup).not.toContain('Records were lost');
  });
});

describe('recorded raises', () => {
  function raise(
    overrides: Partial<TelemetryErrorCapture>,
  ): TelemetryErrorCapture {
    return {
      errorId: 'rec:10',
      grain: 'throw',
      kind: 'fresh',
      source: 'native_call',
      stack: ['user.main', 'user.Classify'],
      stackComplete: true,
      throwCallId: 'rec:3',
      throwCallPathId: 'rec:p2',
      throwFqn: 'user.Classify',
      throwSiteFile: 'main.baml',
      throwSiteLine: 7,
      throwThreadId: 'rec:2',
      value: null,
      valueCid: null,
      valueState: 'not_captured',
      ...overrides,
    };
  }

  it('counts only proven errors and labels boundaries, origins and propagation', () => {
    const evidence = buildEvidence(
      btelTelemetry({
        errors: [
          raise({
            originState: 'fresh',
            propagation: [
              {
                fqn: 'user.main',
                handlerFqn: null,
                kind: 'await',
                raiseId: 'rec:11',
                result: 'unhandled',
                site: {
                  end: 40,
                  file: 'main.baml',
                  line: 12,
                  start: 30,
                  state: 'resolved',
                },
                threadId: 'rec:2',
              },
            ],
            raiseKind: 'native_boundary',
            unwindResult: 'unhandled',
          }),
          raise({
            errorId: 'rec:12',
            kind: 'rethrow',
            originCandidates: 2,
            originState: 'ambiguous',
            raiseKind: 'rethrow',
            source: 'bytecode',
          }),
        ],
        execution: btelExecution({ sourceState: 'stale' }),
      }),
    );
    const overview = renderToStaticMarkup(
      <OverviewTab
        evidence={evidence}
        execution={toExecutionRow(btelExecution())}
        openContext={noop}
        openSpan={noop}
      />,
    );
    expect(overview).toContain('1 error');
    expect(overview).toContain('plus 1 raise with an unproven origin');
    expect(overview).toContain('failed in the native call at');
    expect(overview).toContain('(changed since run)');
    expect(overview).toContain('origin ambiguous');
    expect(overview).toContain('awaited in main');
    expect(overview).toContain('not caught on its thread');
  });
});
