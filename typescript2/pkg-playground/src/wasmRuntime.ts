/**
 * Module-level singleton for the BAML WASM runtime.
 *
 * Key invariants:
 * 1. React component unmount does NOT dispose the runtime (StrictMode safe).
 * 2. Only HMR dispose / page unload own teardown.
 * 3. Every caller grabs the current handle via getRuntime() — no stale refs.
 * 4. Generation tokens prevent stale async from touching freed memory.
 */

import initWasm, { BamlWasmRuntime } from '@b/bridge_wasm';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface RuntimeHandle {
  rt: BamlWasmRuntime;
  gen: number;
}

export interface FetchLogEntry {
  id: number;
  timestamp: number;
  method: string;
  url: string;
  requestHeaders: Record<string, string>;
  requestBody: string;
  status: number | null;
  responseBody: string | null;
  error: string | null;
  durationMs: number | null;
}

export interface EnvVarRequest {
  id: number;
  variable: string;
  resolve: (value: string | undefined) => void;
}

// ---------------------------------------------------------------------------
// Module state
// ---------------------------------------------------------------------------

let current: BamlWasmRuntime | null = null;
let disposed = false;
let initPromise: Promise<RuntimeHandle> | null = null;
let generation = 0;

// Default BAML passed into create() on first init.
let initialCode = '';

// Fetch log — module-level, subscribers notified on changes.
let fetchLogs: FetchLogEntry[] = [];
let nextLogId = 0;
type FetchLogListener = (logs: FetchLogEntry[]) => void;
const fetchListeners = new Set<FetchLogListener>();

// Env vars — pre-set values + pending requests for the UI.
let envVars: Record<string, string> = {};
type EnvVarsListener = (vars: Record<string, string>) => void;
const envListeners = new Set<EnvVarsListener>();

let pendingEnvRequests: EnvVarRequest[] = [];
let nextEnvReqId = 0;
type EnvRequestListener = (reqs: EnvVarRequest[]) => void;
const envReqListeners = new Set<EnvRequestListener>();

// ---------------------------------------------------------------------------
// Fetch log API
// ---------------------------------------------------------------------------

function pushLog(entry: FetchLogEntry): void {
  fetchLogs = [...fetchLogs, entry];
  for (const fn of fetchListeners) fn(fetchLogs);
}

function updateLog(id: number, patch: Partial<FetchLogEntry>): void {
  fetchLogs = fetchLogs.map((e) => (e.id === id ? { ...e, ...patch } : e));
  for (const fn of fetchListeners) fn(fetchLogs);
}

export function subscribeFetchLogs(fn: FetchLogListener): () => void {
  fetchListeners.add(fn);
  fn(fetchLogs); // immediate snapshot
  return () => { fetchListeners.delete(fn); };
}

export function clearFetchLogs(): void {
  fetchLogs = [];
  for (const fn of fetchListeners) fn(fetchLogs);
}

// ---------------------------------------------------------------------------
// Env vars API
// ---------------------------------------------------------------------------

function notifyEnvListeners(): void {
  for (const fn of envListeners) fn(envVars);
}

function notifyEnvReqListeners(): void {
  for (const fn of envReqListeners) fn(pendingEnvRequests);
}

export function setEnvVar(key: string, value: string): void {
  envVars = { ...envVars, [key]: value };
  notifyEnvListeners();
}

export function deleteEnvVar(key: string): void {
  const { [key]: _, ...rest } = envVars;
  envVars = rest;
  notifyEnvListeners();
}

export function subscribeEnvVars(fn: EnvVarsListener): () => void {
  envListeners.add(fn);
  fn(envVars); // immediate snapshot
  return () => { envListeners.delete(fn); };
}

export function subscribeEnvRequests(fn: EnvRequestListener): () => void {
  envReqListeners.add(fn);
  fn(pendingEnvRequests); // immediate snapshot
  return () => { envReqListeners.delete(fn); };
}

/** Resolve a pending env var request. Called from the UI when the user submits a value. */
export function resolveEnvRequest(id: number, value: string | undefined): void {
  const req = pendingEnvRequests.find((r) => r.id === id);
  if (!req) return;

  // Save the value for future lookups (if provided).
  if (value !== undefined) {
    setEnvVar(req.variable, value);
  }

  req.resolve(value);
  pendingEnvRequests = pendingEnvRequests.filter((r) => r.id !== id);
  notifyEnvReqListeners();
}

// ---------------------------------------------------------------------------
// Callbacks wired into the WASM runtime
// ---------------------------------------------------------------------------

async function loggingFetch(
  method: string,
  url: string,
  headersJson: string,
  body: string,
): Promise<{
  status: number;
  headersJson: string;
  url: string;
  bodyPromise: Promise<string>;
}> {
  const id = nextLogId++;
  let parsedHeaders: Record<string, string> = {};
  try { parsedHeaders = JSON.parse(headersJson); } catch {}

  pushLog({
    id,
    timestamp: Date.now(),
    method,
    url,
    requestHeaders: parsedHeaders,
    requestBody: body,
    status: null,
    responseBody: null,
    error: null,
    durationMs: null,
  });

  const start = performance.now();

  try {
    const response = await fetch(url, {
      method,
      headers: parsedHeaders,
      body: method !== 'GET' && method !== 'HEAD' ? body : undefined,
    });

    const elapsed = Math.round(performance.now() - start);
    const responseHeaders: Record<string, string> = {};
    response.headers.forEach((v, k) => { responseHeaders[k] = v; });

    // Body is read lazily by the WASM side via bodyPromise.
    const bodyText = response.text();

    // Update log with status immediately; body when it resolves.
    updateLog(id, { status: response.status, durationMs: elapsed });
    bodyText.then(
      (text) => updateLog(id, { responseBody: text }),
      (err) => updateLog(id, { error: `Body read error: ${err}` }),
    );

    return {
      status: response.status,
      headersJson: JSON.stringify(responseHeaders),
      url: response.url,
      bodyPromise: bodyText,
    };
  } catch (err) {
    const elapsed = Math.round(performance.now() - start);
    const msg = err instanceof Error ? err.message : String(err);
    updateLog(id, { status: 0, error: msg, durationMs: elapsed });

    return {
      status: 0,
      headersJson: '{}',
      url,
      bodyPromise: Promise.resolve(''),
    };
  }
}

function resolveEnv(variable: string): Promise<string | undefined> {
  // Check the pre-set env vars store first.
  if (variable in envVars) {
    return Promise.resolve(envVars[variable]);
  }

  // Create a pending request for the UI to show.
  return new Promise<string | undefined>((resolve) => {
    const id = nextEnvReqId++;
    const req: EnvVarRequest = { id, variable, resolve };
    pendingEnvRequests = [...pendingEnvRequests, req];
    notifyEnvReqListeners();
  });
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/** Set the initial BAML code used when creating the runtime for the first time. */
export function setInitialCode(code: string): void {
  initialCode = code;
}

/** Get (or lazily create) the singleton runtime. */
export async function getRuntime(): Promise<RuntimeHandle> {
  if (current && !disposed) {
    return { rt: current, gen: generation };
  }

  if (!initPromise) {
    const myGen = generation;

    initPromise = (async () => {
      await initWasm();

      // If a newer generation started while we were awaiting, bail.
      if (myGen !== generation) {
        throw new Error('Runtime superseded during init');
      }

      const srcFilesJson = JSON.stringify({ 'main.baml': initialCode });

      const rt = BamlWasmRuntime.create('/baml_src', srcFilesJson, {
        fetch: loggingFetch,
        env: resolveEnv,
      });

      // Double-check generation hasn't moved on.
      if (myGen !== generation) {
        rt.free();
        throw new Error('Runtime superseded during init');
      }

      current = rt;
      disposed = false;
      return { rt, gen: myGen } as RuntimeHandle;
    })().finally(() => {
      initPromise = null;
    });
  }

  return initPromise;
}

/** Is the given generation still current? Use before calling into WASM after any await. */
export function isGenCurrent(gen: number): boolean {
  return gen === generation && !disposed;
}

/** Dispose the current runtime (called from HMR hooks only, NOT from React cleanup). */
export function disposeRuntime(_reason = 'dispose'): void {
  if (current && !disposed) {
    try {
      current.free();
    } finally {
      disposed = true;
    }
  }
  current = null;
  initPromise = null;
  generation++;
}

// ---------------------------------------------------------------------------
// HMR hooks — so the runtime is torn down when *this module* is replaced.
// ---------------------------------------------------------------------------

// Vite
if (import.meta && (import.meta as any).hot) {
  (import.meta as any).hot.dispose(() => {
    disposeRuntime('HMR dispose (Vite)');
  });
}

// Webpack / Next.js
declare const module: any;
if (typeof module !== 'undefined' && module?.hot) {
  module.hot.dispose(() => {
    disposeRuntime('HMR dispose (Webpack)');
  });
}
