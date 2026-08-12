import type { RunStoreDisplayRun } from './run-store-projections';

type TestRunResultSource = Pick<
  RunStoreDisplayRun,
  'testName' | 'result' | 'error' | 'status'
>;

export function collectLatestTestRunResults(
  testRuns: TestRunResultSource[],
  testStartErrors: ReadonlyMap<string, string>,
): Map<string, unknown> {
  const results = new Map<string, unknown>();

  for (const [testName, error] of testStartErrors) {
    results.set(testName, { outcome: 'error', error });
  }

  for (const run of testRuns) {
    if (!run.testName || results.has(run.testName)) continue;

    if (run.result != null) {
      results.set(run.testName, run.result);
    } else if (run.error) {
      results.set(run.testName, { outcome: 'error', error: run.error });
    } else if (run.status === 'cancelled') {
      results.set(run.testName, {
        outcome: 'error',
        error: 'Cancelled',
      });
    }
  }

  return results;
}
