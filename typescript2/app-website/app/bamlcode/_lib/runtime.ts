import {
  createRunStoreClient,
  type RunStoreClient,
  type RunSubscriptionEvent,
  WorkerRuntimePort,
} from '@b/pkg-playground';
import { encodeRunArgs } from '@b/pkg-proto';
import { createRawBamlWorker } from '@/playground/spawnBamlWorker';
import { buildProjectFiles, type GraderCase } from './harness';
import type { Problem } from './types';

/**
 * Deterministic, client-side grading runtime for /bamlcode.
 *
 * ONE shared BexVM worker (module singleton - mirrors learn2's runtime, and
 * never terminated so React StrictMode's mount/unmount/remount can't wedge it)
 * runs a single project (`/workspace/bamlcode`) whose baml_src holds the
 * solver's `solution.baml`, an auto-generated `grader.baml`, and an optional
 * `prelude.baml`. Grading calls `startRun` on each zero-arg grader function and
 * reads the terminal run STATUS: `succeeded` = pass, anything else = fail. No
 * test value is ever decoded, and no API key is required.
 */

export interface LspPos {
  line: number;
  character: number;
}
export interface LspDiagnostic {
  range: { start: LspPos; end: LspPos };
  severity?: number; // 1=Error 2=Warning 3=Info 4=Hint
  message: string;
  code?: string;
}
type DiagHandler = (diags: LspDiagnostic[]) => void;

export type CaseStatus = 'pass' | 'fail' | 'error';
export interface CaseResult {
  index: number;
  fnName: string;
  status: CaseStatus;
  errorClass?: string;
  errorMessage?: string;
}

/** Result of `baml run`: the decoded return value, or an error. */
export interface RunView {
  ok: boolean;
  value?: unknown;
  error?: string;
}

const ROOT = '/workspace';
const ID = 'bamlcode';
const SOLUTION_REL = `${ID}/baml_src/solution.baml`;
const GRADER_REL = `${ID}/baml_src/grader.baml`;
const PRELUDE_REL = `${ID}/baml_src/prelude.baml`;
const RUNNER_REL = `${ID}/baml_src/runner.baml`;
// Sentinel prefix on the panic that carries a `baml run` result, so we can tell
// an intended output from a real error in the user's code.
const RUN_SENTINEL = 'BCRUN:';
const SOLUTION_URI = `file://${ROOT}/${SOLUTION_REL}`;
const PROJECT_FALLBACK = `${ROOT}/${ID}`;

const TERMINAL_RUN_STATUS = new Set([
  'succeeded',
  'failed',
  'cancelled',
  'panicked',
]);

function projectMatches(projectKey: string): boolean {
  return projectKey.split('/').includes(ID);
}

/** Reduce a runtime traceback to its most useful single line. */
function cleanRunError(msg: string): string {
  if (!msg) return 'run failed';
  const lines = msg
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
  // Return the full last line (the console pane scrolls) rather than hard-
  // capping it, so the whole error is readable.
  return lines.at(-1) ?? msg;
}

/** Strip a leading/trailing markdown code fence (```baml ... ```) from an LSP
 * hover so `baml describe` shows a clean signature, not raw markdown. */
function stripHoverFences(text: string): string {
  return text
    .replace(/^\s*```[a-zA-Z]*\n?/, '')
    .replace(/\n?```\s*$/, '')
    .trim();
}

function waitUntil(cond: () => boolean, timeoutMs: number): Promise<void> {
  return new Promise((resolve) => {
    const start = Date.now();
    const tick = () => {
      if (cond() || Date.now() - start > timeoutMs) resolve();
      else setTimeout(tick, 40);
    };
    tick();
  });
}

// ── Shared worker singleton ────────────────────────────────────────────────
interface InternalRuntime {
  onDiag(diags: LspDiagnostic[]): void;
  onUpdateProject(project: string): void;
}

let sharedWorkerPromise: Promise<Worker> | null = null;
let sharedClientPromise: Promise<RunStoreClient> | null = null;
let active: InternalRuntime | null = null;

// Pending generic LSP requests (hover for `baml describe`), keyed by reqId.
const pendingLsp = new Map<number, (result: unknown) => void>();
let nextLspReq = 1;

function flattenHover(contents: unknown): string {
  if (contents == null) return '';
  if (typeof contents === 'string') return contents;
  if (Array.isArray(contents))
    return contents.map(flattenHover).filter(Boolean).join('\n\n');
  const c = contents as { value?: unknown };
  return typeof c.value === 'string' ? c.value : '';
}

function ensureSharedWorker(): Promise<Worker> {
  if (sharedWorkerPromise) return sharedWorkerPromise;
  sharedWorkerPromise = new Promise<Worker>((resolve) => {
    const w = createRawBamlWorker();
    w.addEventListener('message', (e: MessageEvent) => {
      const d = e.data;
      if (d?.type === 'ready') {
        resolve(w);
        return;
      }
      if (d?.type === 'lspDiagnostics') {
        if (d.params?.uri === SOLUTION_URI) {
          active?.onDiag((d.params?.diagnostics ?? []) as LspDiagnostic[]);
        }
        return;
      }
      if (d?.type === 'lspResult') {
        const cb = pendingLsp.get(d.reqId);
        if (cb) {
          pendingLsp.delete(d.reqId);
          cb(d.result ?? null);
        }
        return;
      }
      if (d?.type === 'playgroundNotification') {
        const n = d.notification;
        if (n?.type === 'updateProject' && typeof n.project === 'string') {
          active?.onUpdateProject(n.project);
        }
      }
    });
    w.postMessage({ initialFiles: {}, rootPath: ROOT, type: 'init' });
  });
  return sharedWorkerPromise;
}

function ensureSharedClient(): Promise<RunStoreClient> {
  if (sharedClientPromise) return sharedClientPromise;
  sharedClientPromise = ensureSharedWorker().then((w) =>
    createRunStoreClient(new WorkerRuntimePort(w)),
  );
  return sharedClientPromise;
}

/**
 * `baml describe` for an arbitrary expression/symbol, independent of any solve
 * page (used by the syntax reference). Drops the symbol into a scratch file and
 * returns the LSP hover (resolved type / signature / docs).
 */
export async function describeExpr(expr: string): Promise<string> {
  const e = expr.trim();
  if (!e) return '';
  const w = await ensureSharedWorker();
  const rel = `${ID}/baml_src/describe.baml`;
  const line2 = `  let __x = ${e};`;
  const body = `function __describe() -> int {\n${line2}\n  return 0;\n}\n`;
  w.postMessage({ files: { [rel]: body }, type: 'openFiles' });
  await new Promise((r) => setTimeout(r, 800));
  // Hover on the LAST identifier of the expression (e.g. `stringify` in
  // `baml.json.stringify`, `chars` in `"abc".chars()`), not the first token.
  let lastIdx = 0;
  let lastLen = e.length;
  for (const mm of e.matchAll(/[A-Za-z_]\w*/g)) {
    lastIdx = mm.index ?? 0;
    lastLen = mm[0].length;
  }
  const character = line2.indexOf(e) + lastIdx + Math.floor(lastLen / 2);
  const hover = await lspRequest('textDocument/hover', {
    position: { character, line: 1 },
    textDocument: { uri: `file://${ROOT}/${rel}` },
  });
  // Clear the scratch so it can't affect grading.
  w.postMessage({ files: { [rel]: '' }, type: 'filesChanged' });
  const text = stripHoverFences(
    flattenHover((hover as { contents?: unknown })?.contents),
  );
  return text || `No description found for \`${e}\`.`;
}

async function lspRequest(method: string, params: unknown): Promise<unknown> {
  const w = await ensureSharedWorker();
  const reqId = nextLspReq++;
  return new Promise((resolve) => {
    pendingLsp.set(reqId, resolve);
    w.postMessage({ method, params, reqId, type: 'lspRequest' });
    setTimeout(() => {
      if (pendingLsp.has(reqId)) {
        pendingLsp.delete(reqId);
        resolve(null);
      }
    }, 4000);
  });
}

export interface SolveRuntime {
  /** Push an editor edit (updates diagnostics; marks the project dirty). */
  updateSolution(code: string): void;
  onDiagnostics(cb: DiagHandler): void;
  /** Latest error-severity diagnostics on the solution file. */
  solutionErrors(): LspDiagnostic[];
  /** Grade the given cases; resolves once every run reaches a terminal state. */
  grade(cases: GraderCase[]): Promise<CaseResult[]>;
  /** `baml run`: evaluate a BAML call expression and return its value. */
  runCall(call: string): Promise<RunView>;
  /** `baml describe`: the resolved type/signature of a function (LSP hover). */
  describe(fnName: string): Promise<string>;
  /** Mark this runtime as the one receiving diagnostics/updateProject. */
  activate(): void;
  /** Stop receiving worker notifications (does NOT tear down the shared worker). */
  deactivate(): void;
}

export function createSolveRuntime(
  problem: Problem,
  initialSolution: string,
): SolveRuntime {
  let projectKey = PROJECT_FALLBACK;
  let recompilePending = true;
  let latestDiags: LspDiagnostic[] = [];
  let diagHandler: DiagHandler | undefined;
  let latestCode = initialSolution;

  const internal: InternalRuntime = {
    onDiag(diags) {
      latestDiags = diags;
      diagHandler?.(diags);
    },
    onUpdateProject(project) {
      if (projectMatches(project)) {
        projectKey = project;
        recompilePending = false;
      }
    },
  };
  active = internal;

  // Register this problem's project on the shared worker (openFiles didOpens
  // each file, triggering a compile + updateProject/diagnostics).
  void ensureSharedWorker().then((w) => {
    // NOTE: no baml.toml - the worker didOpens every file as BAML, so a
    // baml.toml poisons the project with parse errors and the bex never builds.
    // The baml_src/ dir alone defines the project.
    const src = buildProjectFiles(problem, initialSolution);
    const files: Record<string, string> = {
      [SOLUTION_REL]: src['solution.baml'],
      [GRADER_REL]: src['grader.baml'],
    };
    if (src['prelude.baml']) files[PRELUDE_REL] = src['prelude.baml'];
    recompilePending = true;
    w.postMessage({ files, type: 'openFiles' });
  });

  type RunOutcome = {
    status: CaseStatus;
    errorClass?: string;
    errorMessage?: string;
  };

  function outcomeFromStatus(
    status: string,
    error?: { class?: string; message?: string } | null,
  ): RunOutcome {
    if (status === 'succeeded') return { status: 'pass' };
    if (status === 'cancelled') {
      return { errorMessage: 'cancelled', status: 'error' };
    }
    // failed | panicked
    return {
      errorClass: error?.class,
      errorMessage: error?.message,
      status: 'fail',
    };
  }

  async function runOne(fnName: string): Promise<RunOutcome> {
    const client = await ensureSharedClient();
    // argsBytes is transferred/detached on postMessage - encode a fresh empty
    // arg map per run.
    const argsBytes = new Uint8Array(encodeRunArgs({}));
    let boundaryId: string;
    try {
      boundaryId = await client.startRun({
        argsBytes,
        functionName: fnName,
        project: projectKey,
      });
    } catch {
      return { errorMessage: 'could not start run', status: 'error' };
    }

    // Read the terminal status straight from the patch/snapshot stream. We
    // deliberately never call client.snapshot() - the website worker's snapshot
    // handler is broken (wasmSnapshotFailed: undefined.length), so relying on
    // it (e.g. on cursorExpired) throws.
    const handle = client.subscribe(boundaryId);
    const iterator = handle.events[Symbol.asyncIterator]();
    const timeout = new Promise<'timeout'>((resolve) =>
      setTimeout(() => resolve('timeout'), 15000),
    );
    try {
      while (true) {
        const next = await Promise.race([iterator.next(), timeout]);
        if (next === 'timeout')
          return { errorMessage: 'timed out', status: 'error' };
        if (next.done) break;
        const event = next.value as RunSubscriptionEvent;
        if (event.type === 'snapshot') {
          const s = event.snapshot;
          if (TERMINAL_RUN_STATUS.has(s.status)) {
            return outcomeFromStatus(s.status, s.error);
          }
        } else if (event.type === 'patch') {
          // Prefer `complete` (it carries the authoritative error/result); a
          // patch can contain BOTH [setStatus, complete], and setStatus alone
          // has no error message.
          const complete = event.patch.changes.find(
            (c) => c.type === 'complete',
          );
          if (complete && complete.type === 'complete') {
            const o = complete.outcome;
            return outcomeFromStatus(
              o.status,
              o.status === 'failed' || o.status === 'panicked' ? o.error : null,
            );
          }
          const term = event.patch.changes.find(
            (c) => c.type === 'setStatus' && TERMINAL_RUN_STATUS.has(c.status),
          );
          if (term && term.type === 'setStatus') {
            return outcomeFromStatus(term.status, null);
          }
        }
      }
    } catch {
      return { errorMessage: 'run failed', status: 'error' };
    } finally {
      void handle.unsubscribe();
    }
    return { errorMessage: 'run did not complete', status: 'error' };
  }

  return {
    activate() {
      active = internal;
    },
    deactivate() {
      if (active === internal) active = null;
      // The shared worker is intentionally NOT terminated - it is reused across
      // problems and remounts.
    },
    async describe(fnName: string): Promise<string> {
      await ensureSharedWorker();
      await waitUntil(() => !recompilePending, 6000);
      const code = latestCode;
      const decl = code.search(new RegExp(`function\\s+${fnName}\\b`));
      if (decl < 0) return `Define \`${fnName}\` first.`;
      const fnStart = code.indexOf(fnName, decl);
      const before = code.slice(0, fnStart);
      const line = before.split('\n').length - 1;
      const character = fnStart - (before.lastIndexOf('\n') + 1);
      const hover = await lspRequest('textDocument/hover', {
        position: { character, line },
        textDocument: { uri: SOLUTION_URI },
      });
      const text = stripHoverFences(
        flattenHover((hover as { contents?: unknown })?.contents),
      );
      return text || `No description available for \`${fnName}\`.`;
    },
    async grade(cases: GraderCase[]): Promise<CaseResult[]> {
      await ensureSharedWorker();
      // Wait for any in-flight recompile to settle so we grade the latest edit,
      // not the previous bex.
      await waitUntil(() => !recompilePending, 8000);

      // A solution that doesn't compile can't be graded - surface the first
      // error instead of running.
      const errs = latestDiags.filter((d) => (d.severity ?? 1) === 1);
      if (errs.length > 0) {
        return cases.map((c) => ({
          errorMessage: errs[0].message,
          fnName: c.fnName,
          index: c.index,
          status: 'error' as const,
        }));
      }

      const results: CaseResult[] = [];
      for (const c of cases) {
        const outcome = await runOne(c.fnName);
        results.push({ fnName: c.fnName, index: c.index, ...outcome });
      }
      return results;
    },
    onDiagnostics(cb: DiagHandler) {
      diagHandler = cb;
      if (latestDiags.length) cb(latestDiags);
    },
    async runCall(call: string): Promise<RunView> {
      const expr = call.trim();
      if (expr === '')
        return { error: 'Enter a call, e.g. TwoSum([2, 7], 9)', ok: false };
      const w = await ensureSharedWorker();
      // The website worker can't read a result value body (no readValue handler),
      // so a runner function panics with the JSON-stringified result and we read
      // it back out of the (inline) error message.
      const wrapper =
        'function bc_run() -> int {\n' +
        `  baml.sys.panic("${RUN_SENTINEL}" + baml.json.stringify((${expr}).to_json()));\n` +
        '}\n';
      recompilePending = true;
      w.postMessage({ files: { [RUNNER_REL]: wrapper }, type: 'openFiles' });
      await waitUntil(() => !recompilePending, 8000);

      const outcome = await runOne('bc_run');
      // Reset the runner so a bad call can't poison later grading.
      w.postMessage({ files: { [RUNNER_REL]: '' }, type: 'filesChanged' });

      // The sentinel lands inside a Rust Debug repr of the panic instance:
      //   ...uncaught throw: Instance { ... "BCRUN:[0,1]" ... }
      // so scan from the sentinel to the closing (unescaped) quote, unescaping
      // as we go, to recover the JSON-stringified result.
      const msg = outcome.errorMessage ?? '';
      const at = msg.indexOf(RUN_SENTINEL);
      if (at >= 0) {
        let raw = '';
        let esc = false;
        for (const ch of msg.slice(at + RUN_SENTINEL.length)) {
          if (esc) {
            raw += ch;
            esc = false;
          } else if (ch === '\\') {
            esc = true;
          } else if (ch === '"') {
            break;
          } else {
            raw += ch;
          }
        }
        try {
          return { ok: true, value: JSON.parse(raw) };
        } catch {
          return { ok: true, value: raw };
        }
      }
      return { error: cleanRunError(msg), ok: false };
    },
    solutionErrors() {
      return latestDiags.filter((d) => (d.severity ?? 1) === 1);
    },
    updateSolution(code: string) {
      latestCode = code;
      recompilePending = true;
      void ensureSharedWorker().then((w) =>
        w.postMessage({
          files: { [SOLUTION_REL]: code },
          type: 'filesChanged',
        }),
      );
    },
  };
}
