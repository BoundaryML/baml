/**
 * Shared BAML (BexVM) worker for the /learn2 live playground.
 *
 * The `new URL('./baml-worker.ts', import.meta.url)` literal must live next to
 * the worker so the bundler emits the worker chunk — hence this factory in
 * `playground/` rather than under `app/learn2/`.
 *
 * We memoise a single worker for the page: the learn2 deck mounts/unmounts the
 * playground slide repeatedly, and there's no `useEffect` cleanup hook to tear a
 * per-mount worker down. One shared, lazily-initialised worker avoids leaks.
 */
/**
 * Create a worker WITHOUT initializing it (no `init` posted). The multi-editor
 * runtime manager drives init/openFiles itself. The `new URL` literal must live
 * here (next to the worker) for the bundler to emit the worker chunk.
 */
export function createRawBamlWorker(): Worker {
  return new Worker(new URL('./baml-worker.ts', import.meta.url), {
    type: 'module',
    name: 'BAML Editor Worker',
  });
}

/**
 * Create a fresh, fully-initialised worker that is NOT memoised — one per
 * caller. Use this when a page mounts more than one playground at once: the
 * shared `getBamlWorker` worker keys every project on the same
 * `baml_src/main.baml`, so two playgrounds sharing it clobber each other's
 * state. An isolated worker keeps each playground's project independent. The
 * caller owns it and must `terminate()` it on unmount.
 */
export function createInitializedBamlWorker(
  initialCode: string,
): Promise<Worker> {
  return new Promise<Worker>((resolve, reject) => {
    let worker: Worker;
    try {
      worker = createRawBamlWorker();
    } catch (err) {
      reject(err instanceof Error ? err : new Error(String(err)));
      return;
    }

    const onReady = (event: MessageEvent) => {
      if (event.data?.type !== 'ready') return;
      worker.removeEventListener('message', onReady);
      resolve(worker);
    };
    worker.addEventListener('message', onReady);

    worker.postMessage({
      type: 'init',
      initialFiles: { 'baml_src/main.baml': initialCode },
      rootPath: '/workspace',
    });
  });
}

let readyPromise: Promise<Worker> | null = null;

export function getBamlWorker(initialCode: string): Promise<Worker> {
  if (readyPromise) return readyPromise;

  readyPromise = new Promise<Worker>((resolve, reject) => {
    let worker: Worker;
    try {
      worker = new Worker(new URL('./baml-worker.ts', import.meta.url), {
        type: 'module',
        name: 'BAML Learn Worker',
      });
    } catch (err) {
      readyPromise = null;
      reject(err instanceof Error ? err : new Error(String(err)));
      return;
    }

    const onReady = (event: MessageEvent) => {
      if (event.data?.type !== 'ready') return;
      worker.removeEventListener('message', onReady);
      resolve(worker);
    };
    worker.addEventListener('message', onReady);

    worker.postMessage({
      type: 'init',
      initialFiles: { 'baml_src/main.baml': initialCode },
      rootPath: '/workspace',
    });
  });

  return readyPromise;
}
