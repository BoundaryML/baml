import {
  awaitRunCompletion,
  createRunStoreClient,
  createValueBodyCache,
  type RunStoreClient,
  type ValueBodyCache,
  WorkerRuntimePort,
} from '@b/pkg-playground';
import { createRawBamlWorker } from '@/playground/spawnBamlWorker';

/**
 * Shared multi-editor BAML runtime: ONE BexVM worker serves many independent
 * editors. Each editor ("cell") gets its own project root under /workspace, so
 * snippets don't clash. Diagnostics, code lenses, and test/testset runs are
 * routed back to the owning cell.
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

export interface CodeLensCommand {
  title: string;
  command: string;
  arguments?: unknown[];
}
export interface CodeLensItem {
  range: { start: LspPos; end: LspPos };
  command?: CodeLensCommand;
}

export interface LspRange {
  start: LspPos;
  end: LspPos;
}
export interface LspHover {
  contents: unknown;
  range?: LspRange;
}
export interface LspInlayHint {
  position: LspPos;
  label: string | Array<{ value?: string; label?: string }>;
  kind?: number;
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

export interface RunResult {
  ok: boolean;
  value?: unknown;
  error?: string;
}

// Serialized test tree (testing.SerializedTestDef): a leaf test, an unexpanded
// lazy testset, or an expanded testset with `items`.
type SerializedNode =
  | { type: 'test' | 'lazyTestSet'; name: string }
  | { name: string; items: SerializedNode[]; loadingTimeMs?: number };

type DiagHandler = (diags: LspDiagnostic[]) => void;

interface Cell {
  id: string;
  mainUri: string;
  onDiag?: DiagHandler;
  /** The runtime's project key (captured from notifications). */
  project?: string;
  /** Latest test-collection generation for this project. */
  generation?: number;
  /** Latest decoded test tree for this project. */
  tree?: SerializedNode[];
  /** Set when the runtime fires `updateProject` for this project — i.e. it has
   *  (re)built the executable "bex". Runs are gated on this; without it the
   *  runtime reports "engine not ready: No bex has been created yet". */
  built?: boolean;
  /** Latest file contents, so a pre-run rebuild re-sends the current code. */
  code?: string;
}

const ROOT = '/workspace';
let workerPromise: Promise<Worker> | null = null;
let nextReqId = 1;
const cells = new Map<string, Cell>();
const pendingCodeLens = new Map<number, (lenses: CodeLensItem[]) => void>();
const pendingLsp = new Map<number, (result: unknown) => void>();

// Test runs go through the shared RunStore client — the same execution path the
// playground's ExecutionPanel uses — layered over the existing worker via
// WorkerRuntimePort (which uses addEventListener, so it coexists with the raw
// diagnostics/codelens listener below). The valueBodyCache fetches the result
// `valueRef` bodies the runtime returns instead of inline values.
interface RunContext {
  client: RunStoreClient;
  valueBodyCache: ValueBodyCache;
}
let runContextPromise: Promise<RunContext> | null = null;

function ensureRunContext(): Promise<RunContext> {
  if (runContextPromise) return runContextPromise;
  runContextPromise = ensureWorker().then((worker) => {
    const client = createRunStoreClient(new WorkerRuntimePort(worker));
    const valueBodyCache = createValueBodyCache(client);
    return { client, valueBodyCache };
  });
  return runContextPromise;
}

function projectMatchesCell(projectKey: string, id: string): boolean {
  return projectKey.split('/').includes(id);
}

function findNode(
  tree: SerializedNode[] | undefined,
  name: string,
): SerializedNode | undefined {
  if (!tree) return undefined;
  for (const node of tree) {
    if (node.name === name) return node;
    if ('items' in node) {
      const found = findNode(node.items, name);
      if (found) return found;
    }
  }
  return undefined;
}

function isTestSetNode(node: SerializedNode): boolean {
  return 'items' in node || node.type === 'lazyTestSet';
}

function leafTestNames(node: SerializedNode): string[] {
  if ('items' in node) return node.items.flatMap(leafTestNames);
  return node.type === 'test' ? [node.name] : [];
}

function waitUntil(cond: () => boolean, timeoutMs: number): Promise<void> {
  return new Promise((resolve) => {
    const start = Date.now();
    const tick = () => {
      if (cond() || Date.now() - start > timeoutMs) resolve();
      else setTimeout(tick, 60);
    };
    tick();
  });
}

function ensureWorker(): Promise<Worker> {
  if (workerPromise) return workerPromise;
  workerPromise = new Promise<Worker>((resolve) => {
    const worker = createRawBamlWorker();
    worker.addEventListener('message', (e: MessageEvent) => {
      const d = e.data;
      if (d?.type === 'ready') {
        resolve(worker);
        return;
      }
      if (d?.type === 'lspDiagnostics') {
        const uri: string | undefined = d.params?.uri;
        const list: LspDiagnostic[] = d.params?.diagnostics ?? [];
        for (const c of cells.values()) {
          if (c.mainUri === uri) c.onDiag?.(list);
        }
        return;
      }
      if (d?.type === 'codeLensResult') {
        const cb = pendingCodeLens.get(d.reqId);
        if (cb) {
          pendingCodeLens.delete(d.reqId);
          cb(d.lenses ?? []);
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
          for (const c of cells.values()) {
            if (projectMatchesCell(n.project, c.id)) {
              c.project = n.project;
              // updateProject fires after the runtime (re)builds the bex.
              c.built = true;
            }
          }
        } else if (
          n?.type === 'testCollectionResult' &&
          typeof n.project === 'string'
        ) {
          for (const c of cells.values()) {
            if (!projectMatchesCell(n.project, c.id)) continue;
            c.generation = n.generation;
            try {
              const json = new TextDecoder().decode(new Uint8Array(n.data));
              c.tree = JSON.parse(json) as SerializedNode[];
            } catch {
              // ignore decode errors
            }
          }
        }
      }
    });
    worker.postMessage({ type: 'init', initialFiles: {}, rootPath: ROOT });
  });
  return workerPromise;
}

async function callOneTest(
  project: string,
  generation: number,
  testName: string,
): Promise<RunResult> {
  const { client, valueBodyCache } = await ensureRunContext();
  let runId: string;
  try {
    runId = await client.startTestRun({ project, generation, testName });
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : 'run failed' };
  }

  // Drain the run to a terminal status and decode its result via the shared
  // RunStore helper — terminal detection, valueRef-body fetch, and decode all
  // live in one place so a protocol change can't silently break this copy.
  const { run, status, value } = await awaitRunCompletion(
    client,
    valueBodyCache,
    runId,
  );
  if (!run) return { ok: false, error: 'run did not complete' };
  if (status === 'cancelled') return { ok: false, error: 'cancelled' };
  // A failed assertion still decodes to a TestReport (outcome 'fail'); only a
  // runtime error / panic leaves no value.
  if (value == null) {
    return { ok: false, error: run.error?.message ?? 'run failed' };
  }
  return { ok: true, value };
}

function lspRequest(
  worker: Worker,
  method: string,
  params: unknown,
): Promise<unknown> {
  const reqId = nextReqId++;
  return new Promise((resolve) => {
    pendingLsp.set(reqId, resolve);
    worker.postMessage({ type: 'lspRequest', reqId, method, params });
    setTimeout(() => {
      if (pendingLsp.has(reqId)) {
        pendingLsp.delete(reqId);
        resolve(null);
      }
    }, 4000);
  });
}

function testPassed(value: unknown): boolean {
  const v = value as
    | { outcome?: string; runs?: Array<{ outcome?: string }> }
    | null
    | undefined;
  if (v?.outcome) return v.outcome === 'pass';
  const runs = v?.runs ?? [];
  return runs.every((r) => !r.outcome || r.outcome === 'pass');
}

export interface CellHandle {
  projectPath: string;
  updateCode(code: string): void;
  onDiagnostics(cb: DiagHandler): void;
  requestCodeLens(): Promise<CodeLensItem[]>;
  /** Run a single test, or all tests in a testset (routed via the test tree). */
  runTest(name: string): Promise<RunResult>;
  hover(line: number, character: number): Promise<LspHover | null>;
  inlayHints(range: LspRange): Promise<LspInlayHint[]>;
  completion(line: number, character: number): Promise<LspCompletionItem[]>;
  dispose(): void;
}

export function registerCell(id: string, initialCode: string): CellHandle {
  const mainRel = `${id}/baml_src/main.baml`;
  const tomlRel = `${id}/baml.toml`;
  const mainUri = `file://${ROOT}/${mainRel}`;
  // The runtime's project key is the ROOT (the dir holding baml.toml / baml_src),
  // e.g. /workspace/cell0 — NOT /workspace/cell0/baml_src. Using the wrong key
  // spins up a phantom second project (churn + cancelled collects).
  const fallbackProject = `${ROOT}/${id}`;
  const cell: Cell = { id, mainUri, code: initialCode };
  cells.set(id, cell);

  void ensureWorker().then((worker) => {
    worker.postMessage({
      type: 'openFiles',
      files: {
        [tomlRel]: `name = "${id}"\n`,
        [mainRel]: initialCode,
      },
    });
  });

  // Force a fresh project re-eval and wait for the runtime to (re)build the bex
  // before a run. Several editors register `openFiles` at page load, and that
  // async build races — earlier projects can be left without a bex, so a run
  // fires into "engine not ready". Re-sending this one cell's file via
  // `filesChanged` (didChange) reliably re-triggers the build + `updateProject`.
  async function ensureBuilt(worker: Worker): Promise<void> {
    cell.built = false;
    worker.postMessage({
      type: 'filesChanged',
      files: { [mainRel]: cell.code ?? initialCode },
    });
    await waitUntil(() => cell.built === true, 6000);
  }

  async function ensureGeneration(worker: Worker): Promise<void> {
    if (cell.generation != null) return;
    worker.postMessage({
      type: 'requestCollectTests',
      project: cell.project ?? fallbackProject,
    });
    await waitUntil(() => cell.generation != null, 5000);
  }

  async function runTestSet(worker: Worker, name: string): Promise<RunResult> {
    const project = cell.project ?? fallbackProject;
    const gen = cell.generation ?? 0;
    let node = findNode(cell.tree, name);
    // Expand lazy testsets so we can enumerate their tests.
    if (node && 'type' in node && node.type === 'lazyTestSet') {
      worker.postMessage({
        type: 'expandTestSet',
        project,
        generation: gen,
        testsetName: name,
      });
      await waitUntil(() => {
        const n = findNode(cell.tree, name);
        return !!n && 'items' in n;
      }, 5000);
      node = findNode(cell.tree, name);
    }
    const leaves = node ? leafTestNames(node) : [];
    if (leaves.length === 0) {
      return { ok: false, error: 'no runnable tests in testset' };
    }
    let passed = 0;
    for (const leaf of leaves) {
      const r = await callOneTest(project, cell.generation ?? 0, leaf);
      if (r.ok && testPassed(r.value)) passed += 1;
    }
    const total = leaves.length;
    return {
      ok: passed === total,
      value: {
        $baml: { type: 'testing.TestSetReport' },
        outcome: passed === total ? 'pass' : 'fail',
        passed,
        failed: total - passed,
        total,
        results: [],
      },
    };
  }

  return {
    projectPath: fallbackProject,
    updateCode(code: string) {
      cell.code = code;
      void ensureWorker().then((worker) =>
        worker.postMessage({
          type: 'filesChanged',
          files: { [mainRel]: code },
        }),
      );
    },
    onDiagnostics(cb: DiagHandler) {
      cell.onDiag = cb;
    },
    requestCodeLens() {
      return ensureWorker().then(
        (worker) =>
          new Promise<CodeLensItem[]>((resolve) => {
            const reqId = nextReqId++;
            pendingCodeLens.set(reqId, resolve);
            worker.postMessage({
              type: 'requestCodeLens',
              uri: mainUri,
              reqId,
            });
            setTimeout(() => {
              if (pendingCodeLens.has(reqId)) {
                pendingCodeLens.delete(reqId);
                resolve([]);
              }
            }, 4000);
          }),
      );
    },
    hover(line: number, character: number) {
      return ensureWorker()
        .then((worker) =>
          lspRequest(worker, 'textDocument/hover', {
            textDocument: { uri: mainUri },
            position: { line, character },
          }),
        )
        .then((r) => (r as LspHover | null) ?? null);
    },
    inlayHints(range: LspRange) {
      return ensureWorker()
        .then((worker) =>
          lspRequest(worker, 'textDocument/inlayHint', {
            textDocument: { uri: mainUri },
            range,
          }),
        )
        .then((r) => (Array.isArray(r) ? (r as LspInlayHint[]) : []));
    },
    completion(line: number, character: number) {
      return ensureWorker()
        .then((worker) =>
          lspRequest(worker, 'textDocument/completion', {
            textDocument: { uri: mainUri },
            position: { line, character },
          }),
        )
        .then((r) => {
          if (Array.isArray(r)) return r as LspCompletionItem[];
          const list = r as { items?: LspCompletionItem[] } | null;
          return list?.items ?? [];
        });
    },
    async runTest(name: string): Promise<RunResult> {
      const worker = await ensureWorker();
      await ensureBuilt(worker);
      await ensureGeneration(worker);
      const node = findNode(cell.tree, name);
      if (node && isTestSetNode(node)) {
        return runTestSet(worker, name);
      }
      return callOneTest(
        cell.project ?? fallbackProject,
        cell.generation ?? 0,
        name,
      );
    },
    dispose() {
      cells.delete(id);
    },
  };
}
