import {
  awaitRunCompletion,
  createRunStoreClient,
  createValueBodyCache,
  type RunStoreClient,
  type ValueBodyCache,
  WorkerRuntimePort,
} from '@b/pkg-playground';
import { createRawBamlWorker } from '@/playground/spawnBamlWorker';

// The runtime behind a live BAML editor (./baml-editor.tsx): a worker holding the compiler and the
// VM as WebAssembly, with the editor's code as its one project. A worker holds one project, so each
// editor has its own; ./live-code.tsx keeps only the editors near the window mounted, which keeps
// the number of workers small.
//
// Adapted from the previous site's learn2/_editor/runtime.ts, which served every editor from one
// worker. The runtime has since become single-project, and what it ran for any editor's `main` was
// the first editor's. What else changed: a run waits for the engine to be rebuilt from the current
// sources (the runtime refuses to start one before that), a cell can run a function as well as a
// test, and a request for an environment variable is answered at once, because these editors have
// nowhere to ask for an API key.

export interface LspPosition {
  line: number;
  character: number;
}
export interface LspRange {
  start: LspPosition;
  end: LspPosition;
}
export interface LspDiagnostic {
  range: LspRange;
  // 1 is an error, 2 a warning, 3 information, and 4 a hint.
  severity?: number;
  message: string;
  code?: string;
}
export interface CodeLensItem {
  range: LspRange;
  command?: { title: string; command: string; arguments?: unknown[] };
}
export interface LspHover {
  contents: unknown;
  range?: LspRange;
}
export interface LspInlayHint {
  position: LspPosition;
  label: string | { value?: string; label?: string }[];
  paddingLeft?: boolean;
  paddingRight?: boolean;
}
export interface LspCompletionItem {
  label: string | { label: string };
  kind?: number;
  detail?: string;
  documentation?: string | { value?: string };
  insertText?: string;
  textEdit?: { newText?: string };
  sortText?: string;
  filterText?: string;
}

/** What a run came to: the value it returned, or why it has none. */
export interface RunResult {
  ok: boolean;
  value?: unknown;
  error?: string;
}

// The runtime's test tree: a test, a testset not yet expanded, or an expanded testset.
type TestNode =
  | { type: 'test' | 'lazyTestSet'; name: string }
  | { name: string; items: TestNode[] };

type DiagnosticsHandler = (diagnostics: LspDiagnostic[]) => void;

const ROOT = '/workspace';
const FILE = 'baml_src/main.baml';
const URI = `file://${ROOT}/${FILE}`;
const DOCUMENT = { textDocument: { uri: URI } };

/** The runtime names a test by its path, `root::<testset>::<test>`. A code lens and a reader
 *  know it by the last part. */
const SCOPE = '::';
export const shortName = (name: string) => name.split(SCOPE).at(-1) ?? name;

/** The node with this name, given in full or by its last part. */
function findNode(
  tree: TestNode[] | undefined,
  name: string,
): TestNode | undefined {
  for (const node of tree ?? []) {
    if (node.name === name || node.name.endsWith(`${SCOPE}${name}`)) {
      return node;
    }
    if ('items' in node) {
      const found = findNode(node.items, name);
      if (found) return found;
    }
  }
  return undefined;
}

const isSet = (node: TestNode) =>
  'items' in node || node.type === 'lazyTestSet';

function leaves(node: TestNode): string[] {
  if ('items' in node) return node.items.flatMap(leaves);
  return node.type === 'test' ? [node.name] : [];
}

/** Resolves when `done` holds, or after `ms` whether it does or not. `each` runs after every
 *  check that fails, so nothing is asked for once the wait is over. A request sent on the way out
 *  would be answered later, and the answer taken for the state of a later edit. */
function waitUntil(
  done: () => boolean,
  ms: number,
  each?: () => void,
): Promise<void> {
  return new Promise((resolve) => {
    const start = Date.now();
    const tick = () => {
      if (done() || Date.now() - start > ms) {
        resolve();
        return;
      }
      each?.();
      setTimeout(tick, 120);
    };
    tick();
  });
}

/** Whether a test's report says it passed. */
function passed(value: unknown): boolean {
  const v = value as
    | { outcome?: string; runs?: { outcome?: string }[] }
    | null
    | undefined;
  if (v?.outcome) return v.outcome === 'pass';
  return (v?.runs ?? []).every((r) => !r.outcome || r.outcome === 'pass');
}

export interface CellHandle {
  update(code: string): void;
  onDiagnostics(handler: DiagnosticsHandler): void;
  /** Waits for the compiler to finish with the code as it is now. Resolves with the first error
   *  if the code doesn't compile, and with null if it is ready to run. */
  check(): Promise<LspDiagnostic | null>;
  codeLenses(): Promise<CodeLensItem[]>;
  /** Runs a function that takes no arguments. */
  runFunction(name: string): Promise<RunResult>;
  /** The names of the tests and testsets at the top of the file. */
  tests(): Promise<string[]>;
  /** Runs one test, or every test in a testset. A testset's value lists its tests. */
  runTest(name: string): Promise<RunResult>;
  hover(at: LspPosition): Promise<LspHover | null>;
  inlayHints(range: LspRange): Promise<LspInlayHint[]>;
  completions(at: LspPosition): Promise<LspCompletionItem[]>;
  /** Ends the cell's worker. */
  dispose(): void;
}

/** Starts a worker with this code as its project. */
export function createCell(initial: string): CellHandle {
  let code = initial;
  let disposed = false;
  let onDiagnostics: DiagnosticsHandler | undefined;
  // The runtime's own key for the project, taken from its notifications.
  let project = ROOT;
  // Whether the engine has been rebuilt from the sources as they are now.
  let current = false;
  // The compiler's errors for the code, and whether they are for the code as it is now.
  let errors: LspDiagnostic[] = [];
  let checked = false;
  let generation: number | undefined;
  let tree: TestNode[] | undefined;
  let nextRequest = 1;
  const pendingLenses = new Map<number, (lenses: CodeLensItem[]) => void>();
  const pendingLsp = new Map<number, (result: unknown) => void>();

  const worker: Promise<Worker> = new Promise((resolve) => {
    const w = createRawBamlWorker();
    w.addEventListener('message', (e: MessageEvent) => {
      const d = e.data;
      if (d?.type === 'ready') {
        resolve(w);
      } else if (d?.type === 'lspDiagnostics') {
        if (d.params?.uri !== URI) return;
        const diagnostics: LspDiagnostic[] = d.params?.diagnostics ?? [];
        errors = diagnostics.filter((x) => (x.severity ?? 1) === 1);
        checked = true;
        onDiagnostics?.(diagnostics);
      } else if (d?.type === 'codeLensResult') {
        pendingLenses.get(d.reqId)?.(d.lenses ?? []);
        pendingLenses.delete(d.reqId);
      } else if (d?.type === 'lspResult') {
        pendingLsp.get(d.reqId)?.(d.result ?? null);
        pendingLsp.delete(d.reqId);
      } else if (d?.type === 'envVarRequest') {
        // A sample that calls a model asks for its API key. There is no place to enter one
        // here, so the answer is that it isn't set, and the run fails with that as its error.
        w.postMessage({
          id: d.id,
          type: 'envVarResponse',
          variable: d.variable,
        });
      } else if (d?.type === 'playgroundNotification') {
        const n = d.notification;
        if (n?.type === 'updateProject') {
          project = n.project;
          current = n.update?.isBexCurrent === true;
        } else if (n?.type === 'testCollectionResult') {
          generation = n.generation;
          try {
            tree = JSON.parse(
              new TextDecoder().decode(new Uint8Array(n.data)),
            ) as TestNode[];
          } catch {
            // An undecodable tree leaves the previous one in place.
          }
        }
      }
    });
    w.postMessage({
      initialFiles: { [FILE]: code },
      rootPath: ROOT,
      type: 'init',
    });
  });

  // The run client over the worker: the same path the playground's own panel runs on.
  const client: Promise<RunStoreClient> = worker.then((w) =>
    createRunStoreClient(new WorkerRuntimePort(w)),
  );

  /** Sends an LSP request. Resolves with null if no answer comes within four seconds. */
  const lsp = (method: string, params: unknown): Promise<unknown> =>
    worker.then(
      (w) =>
        new Promise((resolve) => {
          const reqId = nextRequest++;
          pendingLsp.set(reqId, resolve);
          w.postMessage({ method, params, reqId, type: 'lspRequest' });
          setTimeout(() => {
            if (pendingLsp.delete(reqId)) resolve(null);
          }, 4000);
        }),
    );

  /** Waits for the engine built from the code as it is now, or for the compiler to say the code
   *  has errors, in which case no engine is coming. The runtime reports a project's state when
   *  asked, so this asks until one or the other is so. Resolves with the first error, if any. */
  async function ready(w: Worker): Promise<LspDiagnostic | null> {
    current = false;
    await waitUntil(
      () => current || (checked && errors.length > 0),
      10000,
      () => w.postMessage({ type: 'requestState' }),
    );
    return current ? null : (errors[0] ?? null);
  }

  /** What a run is given back when the code doesn't compile. */
  const refused = (error: LspDiagnostic): RunResult => ({
    error: `line ${error.range.start.line + 1}: ${error.message}`,
    ok: false,
  });

  /** Waits for a started run to finish and reads the value it returned. */
  async function finish(start: Promise<string>): Promise<RunResult> {
    const c = await client;
    let id: string;
    try {
      id = await start;
    } catch (e) {
      return {
        error: e instanceof Error ? e.message : 'run failed',
        ok: false,
      };
    }
    const values: ValueBodyCache = createValueBodyCache(c);
    const { run, status, value } = await awaitRunCompletion(c, values, id);
    if (!run) return { error: 'the run did not finish', ok: false };
    if (status === 'cancelled') return { error: 'cancelled', ok: false };
    if (status !== 'succeeded' && value == null) {
      return { error: run.error?.message ?? 'run failed', ok: false };
    }
    return { ok: true, value };
  }

  async function collected(w: Worker) {
    generation = undefined;
    w.postMessage({ project, type: 'requestCollectTests' });
    await waitUntil(() => generation != null, 6000);
  }

  async function runOneTest(testName: string): Promise<RunResult> {
    const c = await client;
    return finish(
      c.startTestRun({ generation: generation ?? 0, project, testName }),
    );
  }

  async function runSet(w: Worker, name: string): Promise<RunResult> {
    let node = findNode(tree, name);
    if (node && 'type' in node && node.type === 'lazyTestSet') {
      w.postMessage({
        generation: generation ?? 0,
        project,
        testsetName: name,
        type: 'expandTestSet',
      });
      await waitUntil(() => {
        const n = findNode(tree, name);
        return !!n && 'items' in n;
      }, 6000);
      node = findNode(tree, name);
    }
    const tests = node ? leaves(node) : [];
    if (tests.length === 0) {
      return { error: 'the testset has no tests to run', ok: false };
    }
    const results: { name: string; passed: boolean }[] = [];
    for (const test of tests) {
      const result = await runOneTest(test);
      results.push({
        name: shortName(test),
        passed: result.ok && passed(result.value),
      });
    }
    const count = results.filter((r) => r.passed).length;
    return {
      ok: count === tests.length,
      value: {
        outcome: count === tests.length ? 'pass' : 'fail',
        passed: count,
        results,
        total: tests.length,
      },
    };
  }

  return {
    async check() {
      return ready(await worker);
    },
    codeLenses() {
      return worker.then(
        (w) =>
          new Promise<CodeLensItem[]>((resolve) => {
            const reqId = nextRequest++;
            pendingLenses.set(reqId, resolve);
            w.postMessage({ reqId, type: 'requestCodeLens', uri: URI });
            setTimeout(() => {
              if (pendingLenses.delete(reqId)) resolve([]);
            }, 4000);
          }),
      );
    },
    completions(position) {
      return lsp('textDocument/completion', { ...DOCUMENT, position }).then(
        (r) =>
          Array.isArray(r)
            ? (r as LspCompletionItem[])
            : ((r as { items?: LspCompletionItem[] } | null)?.items ?? []),
      );
    },
    dispose() {
      disposed = true;
      void worker.then((w) => w.terminate());
    },
    hover(position) {
      return lsp('textDocument/hover', { ...DOCUMENT, position }).then(
        (r) => (r as LspHover | null) ?? null,
      );
    },
    inlayHints(range) {
      return lsp('textDocument/inlayHint', { ...DOCUMENT, range }).then((r) =>
        Array.isArray(r) ? (r as LspInlayHint[]) : [],
      );
    },
    onDiagnostics(handler) {
      onDiagnostics = handler;
    },
    async runFunction(name) {
      const error = await ready(await worker);
      if (error) return refused(error);
      const c = await client;
      return finish(c.startRun({ args: '{}', functionName: name, project }));
    },
    async runTest(name) {
      const w = await worker;
      const error = await ready(w);
      if (error) return refused(error);
      await collected(w);
      const node = findNode(tree, name);
      if (!node) {
        // A test declared inside a testset exists only once the testset has run.
        return { error: 'is created by its testset: run that', ok: false };
      }
      return isSet(node) ? runSet(w, node.name) : runOneTest(node.name);
    },
    async tests() {
      const w = await worker;
      if (await ready(w)) return [];
      await collected(w);
      return (tree ?? []).map((node) => node.name);
    },
    update(next) {
      code = next;
      current = false;
      checked = false;
      void worker.then((w) => {
        if (!disposed) {
          w.postMessage({ files: { [FILE]: next }, type: 'filesChanged' });
        }
      });
    },
  };
}
