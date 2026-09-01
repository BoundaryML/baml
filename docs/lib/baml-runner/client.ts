type RunnerFiles = Record<string, string>;

export type RunnerTimings = {
  moduleLoadMs?: number;
  wasmDownloadMs?: number;
  wasmInitializationMs?: number;
  sessionInitializationMs?: number;
  runMs?: number;
};

export type RunnerResponse = {
  ok: boolean;
  output?: string;
  error?: string;
  timings?: RunnerTimings;
  manifest?: { runtimeVersion?: string; sourceCommit?: string };
};

let worker: Worker | undefined;
let nextRequestId = 1;
const pending = new Map<
  number,
  { resolve: (value: RunnerResponse) => void; timer: ReturnType<typeof setTimeout> }
>();

function resetWorker(message: string) {
  worker?.terminate();
  worker = undefined;
  for (const { resolve, timer } of pending.values()) {
    clearTimeout(timer);
    resolve({ ok: false, error: message });
  }
  pending.clear();
}

function getWorker() {
  if (worker) return worker;
  worker = new Worker('/baml-runtime/runner-worker.mjs', { type: 'module' });
  worker.addEventListener('message', (event: MessageEvent<RunnerResponse & { id?: number }>) => {
    const id = event.data?.id;
    if (typeof id !== 'number') return;
    const request = pending.get(id);
    if (!request) return;
    clearTimeout(request.timer);
    pending.delete(id);
    request.resolve(event.data);
  });
  worker.addEventListener('error', () => {
    resetWorker('The BAML runtime stopped unexpectedly. Try running again.');
  });
  return worker;
}

function request(
  type: 'run' | 'warm',
  files: RunnerFiles,
  functionName: string,
): Promise<RunnerResponse> {
  const id = nextRequestId++;
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      resetWorker('The BAML runtime did not respond in time. Try running again.');
    }, 40_000);
    pending.set(id, { resolve, timer });
    getWorker().postMessage({ files, functionName, id, type });
  });
}

export function runBaml(files: RunnerFiles, functionName: string) {
  return request('run', files, functionName);
}

export function warmBaml(files: RunnerFiles, functionName: string) {
  return request('warm', files, functionName);
}

export function shouldWarmBaml() {
  const connection = (
    navigator as Navigator & {
      connection?: { effectiveType?: string; saveData?: boolean };
    }
  ).connection;
  if (!connection) return true;
  return !connection.saveData && !/(^|-)2g$/.test(connection.effectiveType ?? '');
}
