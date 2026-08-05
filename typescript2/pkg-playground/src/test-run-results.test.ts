import { describe, expect, it } from 'vitest';

import type { RunStoreDisplayRun } from './run-store-projections';
import { collectLatestTestRunResults } from './test-run-results';

type TestRunResultSource = Pick<
  RunStoreDisplayRun,
  'testName' | 'result' | 'error' | 'status'
>;

describe('collectLatestTestRunResults', () => {
  it('keeps the newest result for each test when runs are newest first', () => {
    const runs: TestRunResultSource[] = [
      testRun('my/test', { outcome: 'pass' }),
      testRun('my/test', { outcome: 'fail' }),
    ];

    expect(collectLatestTestRunResults(runs, new Map()).get('my/test')).toEqual(
      {
        outcome: 'pass',
      },
    );
  });

  it('uses the latest start error instead of an older completed result', () => {
    const runs: TestRunResultSource[] = [
      testRun('my/test', { outcome: 'pass' }),
    ];

    expect(
      collectLatestTestRunResults(
        runs,
        new Map([['my/test', 'Could not start']]),
      ).get('my/test'),
    ).toEqual({
      outcome: 'error',
      error: 'Could not start',
    });
  });

  it('uses the newest run error instead of an older completed result', () => {
    const runs: TestRunResultSource[] = [
      testRun('my/test', null, {
        error: 'Runtime failed',
        status: 'error',
      }),
      testRun('my/test', { outcome: 'pass' }),
    ];

    expect(collectLatestTestRunResults(runs, new Map()).get('my/test')).toEqual(
      {
        outcome: 'error',
        error: 'Runtime failed',
      },
    );
  });

  it('uses the newest cancellation instead of an older completed result', () => {
    const runs: TestRunResultSource[] = [
      testRun('my/test', null, { status: 'cancelled' }),
      testRun('my/test', { outcome: 'pass' }),
    ];

    expect(collectLatestTestRunResults(runs, new Map()).get('my/test')).toEqual(
      {
        outcome: 'error',
        error: 'Cancelled',
      },
    );
  });
});

function testRun(
  testName: string,
  result: RunStoreDisplayRun['result'],
  overrides: Partial<Pick<TestRunResultSource, 'error' | 'status'>> = {},
): TestRunResultSource {
  return {
    testName,
    result,
    error: null,
    status: 'success',
    ...overrides,
  };
}
