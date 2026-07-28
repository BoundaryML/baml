import type { Run } from './worker-protocol';

export function findLatestGraphRunSnapshot(
  runs: Run[],
  selectedFn: string | null,
  selectedProject: string | null,
  preferredBoundaryId?: string | null,
): Run | undefined {
  if (!selectedFn) return undefined;

  let latest: Run | undefined;
  for (const run of runs) {
    if (!isGraphRunCandidate(run, selectedFn, selectedProject)) continue;
    if (preferredBoundaryId && run.boundaryId === preferredBoundaryId) {
      return run;
    }
    if (!latest || compareRunRecency(run, latest) > 0) {
      latest = run;
    }
  }

  return latest;
}

function isGraphRunCandidate(
  run: Run,
  selectedFn: string,
  selectedProject: string | null,
): boolean {
  if (selectedProject && run.request.projectId !== selectedProject) return false;

  if (
    (run.target.kind === 'function' || run.target.kind === 'companion') &&
    run.target.functionName === selectedFn
  ) {
    return true;
  }
  if (
    run.target.kind === 'preview' &&
    run.target.parentFunctionName === selectedFn
  ) {
    return true;
  }

  return run.calls.some((call) => call.functionName === selectedFn);
}

function compareRunRecency(left: Run, right: Run): number {
  return runRecencyMs(left) - runRecencyMs(right);
}

function runRecencyMs(run: Run): number {
  return run.createdAtMs;
}
