import type { Run } from './worker-protocol';

export type GraphRunMatch = {
  run: Run;
  match: 'target' | 'viaCalls';
};

export function findLatestGraphRunSnapshot(
  runs: Run[],
  selectedFn: string | null,
  selectedProject: string | null,
): GraphRunMatch | undefined {
  if (!selectedFn) return undefined;

  let latest: GraphRunMatch | undefined;
  for (const run of runs) {
    const match = graphRunMatch(run, selectedFn, selectedProject);
    if (!match) continue;
    if (!latest || compareRunRecency(run, latest.run) > 0) {
      latest = { run, match };
    }
  }

  return latest;
}

function graphRunMatch(
  run: Run,
  selectedFn: string,
  selectedProject: string | null,
): GraphRunMatch['match'] | null {
  if (selectedProject && run.request.projectId !== selectedProject) return null;

  if (
    (run.target.kind === 'function' || run.target.kind === 'companion') &&
    run.target.functionName === selectedFn
  ) {
    return 'target';
  }
  if (
    run.target.kind === 'preview' &&
    run.target.parentFunctionName === selectedFn
  ) {
    return 'target';
  }

  return run.calls.some((call) => call.functionName === selectedFn)
    ? 'viaCalls'
    : null;
}

function compareRunRecency(left: Run, right: Run): number {
  return runRecencyMs(left) - runRecencyMs(right);
}

function runRecencyMs(run: Run): number {
  return run.createdAtMs;
}
