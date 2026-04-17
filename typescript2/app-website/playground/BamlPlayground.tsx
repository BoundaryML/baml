'use client';

import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Play, Square, AlertCircle, CheckCircle2, Loader2, KeyRound,
  GitBranch, Workflow, Settings, ChevronDown, ChevronRight,
} from 'lucide-react';
import { BamlEditor } from './BamlEditor';
import {
  ReactFlow,
  ReactFlowProvider,
  Controls,
  Background,
  Handle,
  Position,
  useNodesState,
  useEdgesState,
  type Node as RFNode,
  type Edge as RFEdge,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import ELK from 'elkjs/lib/elk.bundled.js';
import { BamlVfs } from './vfs';
import { encodeCallArgs } from './proto/encode';
import { decodeCallResult } from './proto/decode';
import type {
  BamlWasmRuntime as BamlWasmRuntimeType,
  PlaygroundNotification,
  FunctionInfo,
  TestInfo,
  ProjectDiagnostic,
  LspResponse,
} from '@/wasm/bridge_wasm';

// ---------------------------------------------------------------------------
// Types for control flow graph
// ---------------------------------------------------------------------------

interface BamlPlaygroundProps {
  /** Constrain height to fit a container instead of growing freely */
  compact?: boolean;
}

interface CfgNode {
  id: number;
  parentNodeId: number | null;
  logFilterKey: string;
  label: string;
  sourceExpr: number | null;
  nodeType: string;
  isContainer: boolean;
}

interface CfgEdge {
  src: number;
  dst: number;
  label?: string;
}

interface ControlFlowGraph {
  nodes: Record<string, CfgNode>;
  edgesBySrc: Record<string, CfgEdge[]>;
}

// ---------------------------------------------------------------------------
// Default BAML code
// ---------------------------------------------------------------------------

const DEFAULT_BAML = `// BAML Playground - Try editing and running!

client OpenAI {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

enum Sentiment {
  POSITIVE
  NEGATIVE
  NEUTRAL
}

function ClassifySentiment(text: string) -> Sentiment {
  client OpenAI
  prompt #"
    Classify the sentiment of the following text.

    Text: {{text}}

    Respond with exactly one of: POSITIVE, NEGATIVE, or NEUTRAL.
  "#
}

test HappyTest {
  functions [ClassifySentiment]
  args {
    text "I absolutely love this product! It exceeded all my expectations."
  }
}

test SadTest {
  functions [ClassifySentiment]
  args {
    text "This was terrible. Complete waste of money and time."
  }
}
`;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function mapsToRecordsDeep<T>(input: T): T {
  if (input instanceof Map) {
    const obj: Record<string, any> = {};
    for (const [key, value] of input.entries()) {
      obj[String(key)] = mapsToRecordsDeep(value);
    }
    return obj as T;
  }
  if (Array.isArray(input)) {
    return input.map(mapsToRecordsDeep) as T;
  }
  if (input !== null && typeof input === 'object') {
    const obj: Record<string, any> = {};
    for (const [key, value] of Object.entries(input)) {
      obj[key] = mapsToRecordsDeep(value);
    }
    return obj as T;
  }
  return input;
}

type RuntimeStatus = 'loading' | 'initializing' | 'ready' | 'error';

// ---------------------------------------------------------------------------
// Graph Components
// ---------------------------------------------------------------------------

function BaseNode({ data }: { data: { label: string; nodeType: string } }) {
  const isFunction = data.nodeType === 'functionRoot';
  return (
    <div
      className={`px-4 py-2 rounded-md border text-xs font-mono text-center min-w-[140px] ${
        isFunction
          ? 'bg-violet-950/50 border-violet-500 text-violet-200'
          : 'bg-zinc-900 border-zinc-600 text-zinc-200'
      }`}
    >
      <Handle type="target" position={Position.Top} className="!bg-zinc-500 !w-2 !h-2" />
      {data.label}
      <Handle type="source" position={Position.Bottom} className="!bg-zinc-500 !w-2 !h-2" />
    </div>
  );
}

function DiamondNode({ data }: { data: { label: string } }) {
  return (
    <div className="px-4 py-2 rounded-md border bg-amber-950/40 border-amber-500 text-xs font-mono text-amber-200 text-center min-w-[140px] flex items-center gap-1.5 justify-center">
      <Handle type="target" position={Position.Top} className="!bg-zinc-500 !w-2 !h-2" />
      <GitBranch className="h-3 w-3 rotate-180" />
      {data.label}
      <Handle type="source" position={Position.Bottom} className="!bg-zinc-500 !w-2 !h-2" />
    </div>
  );
}

function GroupNode({ data }: { data: { label: string } }) {
  return (
    <div className="w-full h-full rounded-lg border border-dashed border-zinc-600 bg-zinc-900/30 relative">
      <Handle type="target" position={Position.Top} className="!bg-transparent !w-0 !h-0" />
      <div className="absolute -top-3 left-3 px-2 text-[10px] text-zinc-400 bg-zinc-900 rounded">
        {data.label}
      </div>
      <Handle type="source" position={Position.Bottom} className="!bg-transparent !w-0 !h-0" />
    </div>
  );
}

const nodeTypes = { base: BaseNode, diamond: DiamondNode, group: GroupNode };

const elk = new ELK();

async function layoutGraph(
  cfg: ControlFlowGraph,
): Promise<{ nodes: RFNode[]; edges: RFEdge[] }> {
  const cfgNodes = Object.values(cfg.nodes);
  const allEdges: CfgEdge[] = [];
  for (const edges of Object.values(cfg.edgesBySrc)) {
    allEdges.push(...edges);
  }

  const nodeMap = new Map<number, CfgNode>();
  for (const n of cfgNodes) nodeMap.set(n.id, n);

  // Flatten all nodes for ELK — avoids cross-hierarchy edge errors.
  // Container nodes become visual groups in ReactFlow but are peers in ELK.
  const elkNodes = cfgNodes.map((n) => ({
    id: String(n.id),
    width: n.isContainer ? 250 : 180,
    height: n.isContainer ? 80 : 50,
  }));

  const elkEdges = allEdges.map((e, i) => ({
    id: `e-${i}`,
    sources: [String(e.src)],
    targets: [String(e.dst)],
  }));

  const result = await elk.layout({
    id: 'root',
    children: elkNodes,
    edges: elkEdges,
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'DOWN',
      'spacing.nodeNode': '30',
      'spacing.nodeNodeBetweenLayers': '50',
      'elk.edgeRouting': 'ORTHOGONAL',
    },
  });

  // Convert ELK output to ReactFlow nodes
  const rfNodes: RFNode[] = [];
  const posMap = new Map<string, { x: number; y: number }>();

  for (const elkNode of result.children ?? []) {
    const x = elkNode.x ?? 0;
    const y = elkNode.y ?? 0;
    posMap.set(elkNode.id, { x, y });

    const cfgNode = nodeMap.get(Number(elkNode.id));
    const isContainer = cfgNode?.isContainer ?? false;

    rfNodes.push({
      id: elkNode.id,
      position: { x, y },
      data: {
        label: cfgNode?.label ?? elkNode.id,
        nodeType: cfgNode?.nodeType ?? 'otherScope',
      },
      type: isContainer
        ? 'group'
        : cfgNode?.nodeType === 'branchGroup'
          ? 'diamond'
          : 'base',
      style: isContainer
        ? { width: elkNode.width, height: elkNode.height }
        : undefined,
    });
  }

  // Build edges
  const rfEdges: RFEdge[] = allEdges.map((e, i) => {
    const color = e.label === 'true' ? '#4ade80'
      : e.label === 'false' || e.label === 'else' ? '#f87171'
      : '#94a3b8';
    return {
      id: `e-${i}`,
      source: String(e.src),
      target: String(e.dst),
      type: 'smoothstep',
      label: e.label,
      labelStyle: { fill: color, fontSize: 10 },
      style: { stroke: color, strokeWidth: 1.5 },
      animated: false,
    };
  });

  return { nodes: rfNodes, edges: rfEdges };
}

function FlowGraph({ graph }: { graph: ControlFlowGraph }) {
  const [nodes, setNodes, onNodesChange] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);

  useEffect(() => {
    layoutGraph(graph).then(({ nodes: n, edges: e }) => {
      setNodes(n);
      setEdges(e);
    });
  }, [graph, setNodes, setEdges]);

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      nodeTypes={nodeTypes}
      fitView
      nodesDraggable={false}
      nodesConnectable={false}
      proOptions={{ hideAttribution: true }}
      className="!bg-zinc-950"
    >
      <Controls className="!bg-zinc-800 !border-zinc-700 [&>button]:!bg-zinc-800 [&>button]:!border-zinc-700 [&>button]:!text-zinc-300" />
      <Background color="#333" gap={20} />
    </ReactFlow>
  );
}

// ---------------------------------------------------------------------------
// Main Playground Component
// ---------------------------------------------------------------------------

export function BamlPlayground({ compact }: BamlPlaygroundProps = {}) {
  const [code, setCode] = useState(DEFAULT_BAML);
  const [functions, setFunctions] = useState<FunctionInfo[]>([]);
  const [tests, setTests] = useState<TestInfo[]>([]);
  const [diagnostics, setDiagnostics] = useState<ProjectDiagnostic[]>([]);
  const [selectedFn, setSelectedFn] = useState<string | null>(null);
  const [argsJson, setArgsJson] = useState('{\n  "text": "baml is cool I guess"\n}');
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [apiKey, setApiKey] = useState('');
  const [status, setStatus] = useState<RuntimeStatus>('loading');
  const [statusMsg, setStatusMsg] = useState('Loading WASM runtime...');
  const [fetchLog, setFetchLog] = useState<string | null>(null);
  const [graph, setGraph] = useState<ControlFlowGraph | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const runtimeRef = useRef<BamlWasmRuntimeType | null>(null);
  const vfsRef = useRef<BamlVfs | null>(null);
  const projectPathRef = useRef<string | null>(null);
  const fileVersionRef = useRef(1);
  const pendingResponsesRef = useRef(new Map<number | string, (r: LspResponse) => void>());
  const nextIdRef = useRef(1);
  const codeRef = useRef(code);
  codeRef.current = code;
  const apiKeyRef = useRef(apiKey);
  apiKeyRef.current = apiKey;

  // Initialize WASM runtime - bypass webpack to avoid wasm-bindgen closure corruption
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        setStatusMsg('Loading WASM module (~7MB)...');
        // Load WASM bridge directly from public/ to bypass webpack bundling.
        // Webpack transforms break wasm-bindgen's internal closure table.
        const wasm: any = await import(/* webpackIgnore: true */ '/wasm/bridge_wasm.js');
        await wasm.default();
        if (cancelled) return;

        setStatus('initializing');
        setStatusMsg('Creating BAML runtime...');

        const vfs = new BamlVfs('/workspace');
        vfs.setFiles({ 'baml_src/main.baml': codeRef.current });
        vfsRef.current = vfs;

        const runtime = wasm.BamlWasmRuntime.create(
          {
            fetch: async (
              callId: number,
              method: string,
              url: string,
              headersJson: string,
              body: string,
            ) => {
              const headers = JSON.parse(headersJson);
              setFetchLog(`${method} ${url}`);
              const res = await fetch(url, {
                method,
                headers,
                body: method !== 'GET' && method !== 'HEAD' ? body : undefined,
              });
              const resHeaders: Record<string, string> = {};
              res.headers.forEach((v, k) => {
                resHeaders[k] = v;
              });
              return {
                status: res.status,
                headersJson: JSON.stringify(resHeaders),
                url: res.url,
                bodyPromise: res.text(),
              };
            },
            env: (variable: string) => {
              if (variable === 'OPENAI_API_KEY') {
                return apiKeyRef.current || undefined;
              }
              return undefined;
            },
            lsp_send_notification: () => {},
            lsp_send_response: (response: LspResponse) => {
              response = mapsToRecordsDeep(response);
              const resolve = pendingResponsesRef.current.get(response.id);
              if (resolve) {
                pendingResponsesRef.current.delete(response.id);
                resolve(response);
              }
            },
            lsp_make_request: () => {},
            playground_send_notification: (notification: PlaygroundNotification) => {
              notification = mapsToRecordsDeep(notification);
              if (notification.type === 'updateProject') {
                projectPathRef.current = notification.project;
                setFunctions((notification.update.functions ?? []).filter(
                  (fn: FunctionInfo) => !fn.name.includes('$'),
                ));
                setTests(notification.update.tests ?? []);
                setDiagnostics(notification.update.diagnostics ?? []);
                // Auto-select first function if nothing selected yet
                setSelectedFn((prev) => {
                  if (prev) return prev;
                  const fns = (notification.update.functions ?? []).filter(
                    (fn: FunctionInfo) => !fn.name.includes('$'),
                  );
                  if (fns.length === 0) return null;
                  const fn = fns[0]!;
                  // Also load test args if available
                  const test = (notification.update.tests ?? []).find(
                    (t: TestInfo) => t.functionName === fn.name,
                  );
                  if (test) {
                    try {
                      setArgsJson(JSON.stringify(JSON.parse(test.argsJson), null, 2));
                    } catch {
                      setArgsJson(test.argsJson);
                    }
                  }
                  return fn.name;
                });
              } else if (notification.type === 'controlFlowGraphResult') {
                if (notification.graph) {
                  setGraph(notification.graph as any);
                }
              }
            },
          },
          vfs.wasmVfs,
        );

        if (cancelled) {
          runtime.free();
          return;
        }
        runtimeRef.current = runtime;

        // Initialize LSP protocol
        setStatusMsg('Initializing compiler...');
        await sendLspRequest(runtime, 'initialize', {
          processId: null,
          rootUri: 'file:///workspace',
          capabilities: {},
          workspaceFolders: [{ uri: 'file:///workspace', name: 'workspace' }],
        });

        runtime.handleLspNotification({ method: 'initialized', params: {} });

        runtime.handleLspNotification({
          method: 'textDocument/didOpen',
          params: {
            textDocument: {
              uri: 'file:///workspace/baml_src/main.baml',
              languageId: 'baml',
              version: 1,
              text: codeRef.current,
            },
          },
        });

        runtime.requestPlaygroundState();

        if (!cancelled) {
          setStatus('ready');
          setStatusMsg('Ready');
        }
      } catch (err) {
        if (!cancelled) {
          setStatus('error');
          setStatusMsg(`Error: ${err instanceof Error ? err.message : String(err)}`);
        }
      }
    })();

    return () => {
      cancelled = true;
      if (runtimeRef.current) {
        runtimeRef.current.free();
        runtimeRef.current = null;
      }
    };
  }, []);

  function sendLspRequest(
    runtime: BamlWasmRuntimeType,
    method: string,
    params: any,
  ): Promise<LspResponse> {
    return new Promise((resolve) => {
      const id = nextIdRef.current++;
      pendingResponsesRef.current.set(id, resolve);
      runtime.handleLspRequest({ id, method, params });
    });
  }

  // Request graph when function is selected
  useEffect(() => {
    const runtime = runtimeRef.current;
    const project = projectPathRef.current;
    if (!runtime || !project || !selectedFn) return;
    try {
      runtime.requestControlFlowGraph(project, selectedFn);
    } catch {
      // WASM function may not be ready or signature may differ
    }
  }, [selectedFn, status]);

  // Handle code changes with debounce
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleCodeChange = useCallback((newCode: string) => {
    setCode(newCode);
    codeRef.current = newCode;

    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      const runtime = runtimeRef.current;
      const vfs = vfsRef.current;
      if (!runtime || !vfs) return;

      vfs.setFiles({ 'baml_src/main.baml': newCode });

      const version = ++fileVersionRef.current;
      runtime.handleLspNotification({
        method: 'textDocument/didChange',
        params: {
          textDocument: {
            uri: 'file:///workspace/baml_src/main.baml',
            version,
          },
          contentChanges: [{ text: newCode }],
        },
      });

      runtime.requestPlaygroundState();
    }, 300);
  }, []);

  const handleSelectTest = useCallback((test: TestInfo) => {
    setSelectedFn(test.functionName);
    try {
      const parsed = JSON.parse(test.argsJson);
      setArgsJson(JSON.stringify(parsed, null, 2));
    } catch {
      setArgsJson(test.argsJson);
    }
  }, []);

  const handleRun = useCallback(async () => {
    const runtime = runtimeRef.current;
    if (!runtime || !selectedFn) return;

    if (!apiKeyRef.current) {
      setError('Please enter your OpenAI API key first.');
      return;
    }

    setRunning(true);
    setResult(null);
    setError(null);
    setFetchLog(null);

    try {
      const project = projectPathRef.current || '/workspace/baml_src';
      const args = JSON.parse(argsJson);
      const argsProto = encodeCallArgs(args);
      const resultBytes = await runtime.callFunction(
        Date.now() % 1000000,
        project,
        selectedFn,
        argsProto,
      );
      const decoded = decodeCallResult(
        new Uint8Array(resultBytes),
        (key, handleType, typeName) => ({ key: key.toString(), handleType, typeName }) as any,
      );
      setResult(JSON.stringify(decoded, null, 2));
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
      setFetchLog(null);
    }
  }, [selectedFn, argsJson]);

  const hasErrors = diagnostics.some((d) => d.severity === 'error');

  return (
    <div className={`flex flex-col gap-3 w-full ${compact ? 'h-full overflow-hidden' : ''}`}>
      {/* Settings bar */}
      <div className="flex items-center gap-3 flex-wrap">
        <StatusBadge status={status} message={statusMsg} />

        <button
          className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
          onClick={() => setSettingsOpen(!settingsOpen)}
        >
          <Settings className="h-3 w-3" />
          Settings
          {settingsOpen ? <ChevronDown className="h-3 w-3" /> : <ChevronRight className="h-3 w-3" />}
        </button>

        {/* Functions inline */}
        {functions.length > 0 && (
          <div className="flex items-center gap-1.5 ml-auto">
            <span className="text-xs text-muted-foreground">Function:</span>
            {functions.map((fn) => (
              <button
                key={fn.name}
                className={`px-2 py-0.5 text-xs rounded transition-colors ${
                  selectedFn === fn.name
                    ? 'bg-primary/10 text-primary font-medium'
                    : 'bg-muted hover:bg-muted/80 text-muted-foreground'
                }`}
                onClick={() => {
                  setSelectedFn(fn.name);
                  const test = tests.find((t) => t.functionName === fn.name);
                  if (test) {
                    try {
                      setArgsJson(JSON.stringify(JSON.parse(test.argsJson), null, 2));
                    } catch {
                      setArgsJson(test.argsJson);
                    }
                  }
                }}
              >
                {fn.name}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Collapsible settings */}
      {settingsOpen && (
        <Card className="p-3 flex flex-col sm:flex-row gap-3">
          <div className="flex-1">
            <label className="text-xs font-medium text-muted-foreground flex items-center gap-1.5 mb-1.5">
              <KeyRound className="h-3 w-3" />
              OpenAI API Key
            </label>
            <input
              type="password"
              className="w-full px-3 py-1.5 text-sm border border-border rounded-md bg-background outline-none focus:ring-1 focus:ring-ring"
              placeholder="sk-..."
              value={apiKey}
              onChange={(e) => {
                setApiKey(e.target.value);
                apiKeyRef.current = e.target.value;
              }}
            />
          </div>
          {tests.length > 0 && (
            <div>
              <label className="text-xs font-medium text-muted-foreground mb-1.5 block">
                Load Test
              </label>
              <div className="flex flex-wrap gap-1">
                {tests.map((test) => (
                  <button
                    key={test.name}
                    className="px-2 py-1 text-xs rounded bg-muted hover:bg-muted/80 text-muted-foreground transition-colors"
                    onClick={() => handleSelectTest(test)}
                  >
                    {test.name}
                  </button>
                ))}
              </div>
            </div>
          )}
        </Card>
      )}

      {/* Main content: Editor + Right panel */}
      <div className={`flex flex-col lg:flex-row gap-4 ${compact ? 'flex-1 min-h-0' : 'min-h-[700px]'}`}>
        {/* Editor */}
        <div className="flex-1 min-w-0 flex flex-col gap-3">
          <Card className={`flex-1 flex flex-col overflow-hidden ${compact ? 'min-h-0' : 'min-h-[500px]'}`}>
            <BamlEditor
              value={code}
              onChange={handleCodeChange}
              disabled={status !== 'ready'}
            />
          </Card>

          {diagnostics.length > 0 && (
            <Card className="p-3 max-h-32 overflow-auto">
              <div className="space-y-1">
                {diagnostics.map((d, i) => (
                  <div
                    key={i}
                    className={`text-xs font-mono flex items-start gap-2 ${
                      d.severity === 'error'
                        ? 'text-destructive'
                        : 'text-yellow-600 dark:text-yellow-400'
                    }`}
                  >
                    <AlertCircle className="h-3 w-3 mt-0.5 shrink-0" />
                    <span>{d.message}</span>
                  </div>
                ))}
              </div>
            </Card>
          )}
        </div>

        {/* Right panel: Run + Graph stacked */}
        {!compact && (
        <div className="w-full lg:w-[520px] flex flex-col gap-3">
          {/* Run section */}
          {selectedFn && (
            <>
              <Card className="p-3">
                <label className="text-xs font-medium text-muted-foreground mb-2 block">
                  Arguments for {selectedFn}
                </label>
                <div className="flex items-center gap-1.5 px-3 py-2 text-xs font-mono border border-border rounded-md bg-background">
                  <span className="text-muted-foreground shrink-0">{'{ "text": "'}</span>
                  <input
                    type="text"
                    className="flex-1 min-w-0 bg-transparent outline-none text-xs font-mono"
                    value={(() => { try { return JSON.parse(argsJson).text ?? ''; } catch { return ''; } })()}
                    onChange={(e) => setArgsJson(JSON.stringify({ text: e.target.value }, null, 2))}
                    spellCheck={false}
                  />
                  <span className="text-muted-foreground shrink-0">{'" }'}</span>
                </div>
                <div className="flex gap-2 mt-2">
                  <Button
                    size="sm"
                    onClick={handleRun}
                    disabled={running || hasErrors || status !== 'ready'}
                    className="gap-1.5"
                  >
                    {running ? (
                      <>
                        <Loader2 className="h-3 w-3 animate-spin" />
                        Running...
                      </>
                    ) : (
                      <>
                        <Play className="h-3 w-3" />
                        Run
                      </>
                    )}
                  </Button>
                  {!apiKey && (
                    <button
                      className="text-xs text-muted-foreground hover:text-foreground transition-colors"
                      onClick={() => setSettingsOpen(true)}
                    >
                      Set API key in Settings
                    </button>
                  )}
                </div>
              </Card>

              {fetchLog && (
                <Card className="p-3">
                  <div className="text-xs text-muted-foreground flex items-center gap-2">
                    <Loader2 className="h-3 w-3 animate-spin" />
                    {fetchLog}
                  </div>
                </Card>
              )}

              {(result || error) && (
                <Card className="p-3 overflow-auto max-h-[400px]">
                  <h3 className="text-xs font-medium text-muted-foreground mb-2 flex items-center gap-1.5">
                    {error ? (
                      <>
                        <AlertCircle className="h-3 w-3 text-destructive" />
                        Error
                      </>
                    ) : (
                      <>
                        <CheckCircle2 className="h-3 w-3 text-green-600" />
                        Result
                      </>
                    )}
                  </h3>
                  <pre
                    className={`text-xs font-mono whitespace-pre-wrap break-words ${
                      error ? 'text-destructive' : ''
                    }`}
                  >
                    {error || result}
                  </pre>
                </Card>
              )}
            </>
          )}

          {/* Graph section — always visible below run */}
          {selectedFn && (
            <Card className="flex-1 min-h-[300px] overflow-hidden">
              <div className="border-b border-border bg-muted/50 px-3 py-1.5 flex items-center gap-1.5">
                <Workflow className="h-3 w-3 text-muted-foreground" />
                <span className="text-xs font-medium text-muted-foreground">Control Flow</span>
              </div>
              {graph ? (
                <ReactFlowProvider>
                  <FlowGraph graph={graph} />
                </ReactFlowProvider>
              ) : (
                <div className="flex items-center justify-center h-full text-xs text-muted-foreground py-8">
                  No graph available
                </div>
              )}
            </Card>
          )}
        </div>
        )}
      </div>
    </div>
  );
}

function StatusBadge({ status, message }: { status: RuntimeStatus; message: string }) {
  return (
    <div
      className={`flex items-center gap-1.5 text-xs px-2 py-0.5 rounded-full ${
        status === 'ready'
          ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
          : status === 'error'
            ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
            : 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
      }`}
    >
      {status === 'loading' || status === 'initializing' ? (
        <Loader2 className="h-3 w-3 animate-spin" />
      ) : status === 'ready' ? (
        <CheckCircle2 className="h-3 w-3" />
      ) : (
        <AlertCircle className="h-3 w-3" />
      )}
      {message}
    </div>
  );
}
