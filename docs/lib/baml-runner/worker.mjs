import { createSession, RunTimeout } from './driver.mjs';
import { formatValue } from './outbound.mjs';
import { BamlVfs } from './vfs.mjs';

let runtimePromise;
const sessions = new Map();

async function sha256(bytes) {
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

async function projectKey(files) {
  const canonical = JSON.stringify(
    Object.entries(files).sort(([left], [right]) => left.localeCompare(right)),
  );
  return sha256(new TextEncoder().encode(canonical));
}

async function loadRuntime() {
  const moduleStarted = performance.now();
  const manifestResponse = await fetch('/baml-runtime/manifest.json', {
    cache: 'no-cache',
  });
  if (!manifestResponse.ok) {
    throw new Error(`runtime manifest returned HTTP ${manifestResponse.status}`);
  }
  const manifest = await manifestResponse.json();
  const runtimeModule = await import(manifest.module);
  const moduleLoadMs = performance.now() - moduleStarted;

  const downloadStarted = performance.now();
  const wasmResponse = await fetch(manifest.wasm, { cache: 'force-cache' });
  if (!wasmResponse.ok) {
    throw new Error(`BAML runtime returned HTTP ${wasmResponse.status}`);
  }
  const wasmBytes = new Uint8Array(await wasmResponse.arrayBuffer());
  const wasmDownloadMs = performance.now() - downloadStarted;
  const actualDigest = await sha256(wasmBytes);
  if (actualDigest !== manifest.sha256) {
    throw new Error('BAML runtime digest does not match its manifest');
  }

  const initializationStarted = performance.now();
  await runtimeModule.default({ module_or_path: wasmBytes });
  const wasmInitializationMs = performance.now() - initializationStarted;

  return {
    manifest,
    wasm: runtimeModule,
    timings: { moduleLoadMs, wasmDownloadMs, wasmInitializationMs },
  };
}

async function getRuntime() {
  runtimePromise ??= loadRuntime().catch((error) => {
    runtimePromise = undefined;
    throw error;
  });
  return runtimePromise;
}

async function getSession(files) {
  const key = await projectKey(files);
  let pending = sessions.get(key);
  if (!pending) {
    pending = (async () => {
      const loaded = await getRuntime();
      const started = performance.now();
      const session = await createSession(loaded.wasm, BamlVfs, files, {
        root: `/docs-examples/${key}`,
      });
      return {
        ...loaded,
        session,
        sessionInitializationMs: performance.now() - started,
      };
    })().catch((error) => {
      sessions.delete(key);
      throw error;
    });
    sessions.set(key, pending);
  }
  return pending;
}

self.addEventListener('message', async (event) => {
  const { files, functionName = 'main', id, type } = event.data ?? {};
  const respond = (message) => self.postMessage({ id, ...message });

  if (!id || !files || (type !== 'warm' && type !== 'run')) return;

  try {
    const ready = await getSession(files);
    const base = {
      manifest: {
        runtimeVersion: ready.manifest.runtimeVersion,
        sourceCommit: ready.manifest.sourceCommit,
      },
      timings: {
        ...ready.timings,
        sessionInitializationMs: ready.sessionInitializationMs,
      },
    };
    if (type === 'warm') {
      respond({ ok: true, warmed: true, ...base });
      return;
    }

    const runStarted = performance.now();
    const result = await ready.session.run(functionName, { timeoutMs: 30_000 });
    const runMs = performance.now() - runStarted;
    if (result.status !== 'succeeded') {
      respond({
        ok: false,
        error: result.error?.message ?? `run ${result.status}`,
        timings: { ...base.timings, runMs },
      });
      return;
    }
    respond({
      ok: true,
      output: formatValue(result.value),
      ...base,
      timings: { ...base.timings, runMs },
    });
  } catch (error) {
    respond({
      ok: false,
      error:
        error instanceof RunTimeout
          ? 'The run took too long and was cancelled.'
          : (error?.message ?? String(error)),
    });
  }
});
