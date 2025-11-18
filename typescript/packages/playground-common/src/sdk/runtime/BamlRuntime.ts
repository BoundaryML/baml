/**
 * Real BAML Runtime Implementation
 *
 * Wraps the WASM runtime and implements the BamlRuntimeInterface.
 * This is the production runtime that uses the actual BAML compiler.
 *
 * Key responsibilities:
 * - Load and initialize WASM module
 * - Create WasmProject from BAML files
 * - Extract workflows, functions, diagnostics, and generated files
 * - Execute workflows via WASM runtime
 */

import {
  WasmControlFlowNodeType,
  WasmFunctionKind,
  type WasmProject,
  type WasmRuntime,
  type WasmDiagnosticError,
  type WasmFunction,
  type WasmTestCase,
  type WasmError,
  type WasmSpan,
  type WasmTestResponse,
  type WasmControlFlowGraph,
  type WasmControlFlowNode,
} from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';
import type {
  BamlRuntimeInterface,
  CursorPosition,
  CursorNavigationResult,
  EntityAtPosition,
  ExecutionOptions,
} from './BamlRuntimeInterface';
import type {
  TestCaseInput,
  BAMLFile,
  BAMLTest,
} from '../types';
import type { DiagnosticError, GeneratedFile } from '../atoms/core.atoms';
import { vscode } from '../../shared/baml-project-panel/vscode';

// Import unified types and adapter from interface layer
import {
  WasmTypeAdapter,
  type FunctionWithCallGraph,
  type FunctionMetadata,
  type TestCaseMetadata,
  type CallGraphNode,
  type GraphNode,
  type GraphEdge,
  type BlockType,
  type PromptInfo,
  type RichExecutionEvent,
  type TestExecutionContext,
} from '../interface';





// Type for the WASM module that contains all exports
type BamlWasmModule = typeof import('@gloo-ai/baml-schema-wasm-web/baml_schema_build');

// // Type for WASM diagnostic error objects
// type WasmDiagnosticErrorObject = {
//   type?: string;
//   message?: string;
//   file_path?: string;
//   line?: number;
//   column?: number;
// };

// Type for WASM generator output
type WasmGeneratorOutput = {
  output_dir: string;
  files: Array<{
    path_in_output_dir: string;
    contents: string;
  }>;
};

type RichWasmFunction = WasmFunction & { function_type: WasmFunctionKind };

// Type for test execution callbacks
type WasmPartialResponse = unknown; // The partial response shape varies
type WasmNotification = { variable_name?: string; channel_name?: string; block_name?: string; is_stream: boolean; value: string };

// Type for test result from run_tests
// type WasmTestResult = {
//   func_test_pair: () => { function_name: string; test_name: string };
//   status: () => number; // TestStatus enum value
//   parse_output: () => unknown;
//   raw_output: () => string;
//   llm_output_text: () => string;
// };

// ============================================================================
// Module-Level WASM Cache
// ============================================================================

/**
 * WASM module cache - loaded once and reused across all runtime instances
 * This prevents reloading the entire WASM module on every file change
 */
let wasmModuleCache: BamlWasmModule | null = null;

/**
 * Load WASM module once and cache it
 * Subsequent calls return the cached module immediately
 */
async function getWasmModule(): Promise<BamlWasmModule> {
  if (!wasmModuleCache) {
    console.log('[BamlRuntime] Loading WASM module for the first time...');
    wasmModuleCache = await import('@gloo-ai/baml-schema-wasm-web/baml_schema_build');

    // CRITICAL: Initialize callback bridge ONCE when module is loaded
    // This enables AWS/GCP credential loading
    console.log('[BamlRuntime] Initializing WASM callback bridge');
    wasmModuleCache.init_js_callback_bridge(vscode.loadAwsCreds, vscode.loadGcpCreds);

    console.log('[BamlRuntime] WASM module loaded and cached ✓');
  }
  console.log('loaded wasm from cache');

  return wasmModuleCache;
}

type ControlFlowArtifacts = {
  callGraph: CallGraphNode;
  callGraphDepth: number;
  nodes: GraphNode[];
  edges: GraphEdge[];
  rootType: FunctionMetadata['type'];
};

export type ControlFlowOptions = {
  rootName: string;
  rootType: FunctionMetadata['type'];
  llmClient?: string;
  timestamp: number;
};

export function createFallbackControlFlowArtifacts(
  metadata: FunctionMetadata,
  timestamp: number
): ControlFlowArtifacts {
  const nodeType: GraphNode['type'] =
    metadata.type === 'llm_function'
      ? 'llm_function'
      : metadata.type === 'workflow'
        ? 'group'
        : 'function';
  const callGraphType: CallGraphNode['type'] = (() => {
    if (metadata.type === 'llm_function') return 'llm_function';
    if (metadata.type === 'workflow') return 'block';
    return 'function';
  })();
  const callGraph: CallGraphNode = {
    id: metadata.name,
    type: callGraphType,
    children: [],
    span: metadata.span,
  };

  const fallbackNode: GraphNode = {
    id: metadata.name,
    type: nodeType,
    label: metadata.name,
    functionName: metadata.name,
    codeHash: '',
    lastModified: timestamp,
    llmClient: metadata.clientName,
  };

  return {
    callGraph,
    callGraphDepth: 1,
    nodes: [fallbackNode],
    edges: [],
    rootType: metadata.type,
  };
}

export function buildControlFlowArtifacts(
  graph: WasmControlFlowGraph,
  adapter: WasmTypeAdapter,
  options: ControlFlowOptions
): ControlFlowArtifacts | null {
  const nodes = graph.nodes ?? [];
  if (nodes.length === 0) {
    return null;
  }

  const hasStructure = nodes.some(
    (node) => node.node_type !== WasmControlFlowNodeType.FunctionRoot
  );
  const normalizedRootType: FunctionMetadata['type'] = hasStructure
    ? 'workflow'
    : options.rootType;

  const nodeById = new Map<number, WasmControlFlowNode>();
  const nodeKeyById = new Map<number, string>();
  const childrenByParent = new Map<number, WasmControlFlowNode[]>();

  for (const node of nodes) {
    nodeById.set(node.id, node);
    const key = node.parent_id === undefined
      ? options.rootName
      : deriveNodeKey(node, options.rootName);
    nodeKeyById.set(node.id, key);
    if (node.parent_id !== undefined) {
      const siblings = childrenByParent.get(node.parent_id) ?? [];
      siblings.push(node);
      childrenByParent.set(node.parent_id, siblings);
    }
  }

  const rootNode = nodes.find((node) => node.parent_id === undefined);
  if (!rootNode) {
    return null;
  }

  const toCallGraphNode = (node: WasmControlFlowNode): CallGraphNode => {
    const children = (childrenByParent.get(node.id) ?? []).map((child) => toCallGraphNode(child));
    return {
      id: nodeKeyById.get(node.id)!,
      type: mapNodeTypeToCallGraphType(node.node_type, normalizedRootType),
      blockType: mapNodeTypeToBlockType(node.node_type),
      annotation: node.label || undefined,
      children,
      span: adapter.convertSpan(node.span),
    };
  };

  const callGraph = toCallGraphNode(rootNode);
  const callGraphDepth = computeCallGraphDepth(callGraph);

  const graphNodes: GraphNode[] = nodes.map((node) => {
    const id = nodeKeyById.get(node.id)!;
    return {
      id,
      type: mapNodeTypeToGraphNodeType(node.node_type, normalizedRootType),
      label: node.label || id,
      functionName: options.rootName,
      parent: node.parent_id !== undefined ? nodeKeyById.get(node.parent_id) : undefined,
      codeHash: '',
      lastModified: options.timestamp,
      llmClient: node.node_type === WasmControlFlowNodeType.FunctionRoot ? options.llmClient : undefined,
      metadata: {
        wasmNodeId: node.id,
        lexicalId: node.lexical_id,
        controlFlowType: WasmControlFlowNodeType[node.node_type] ?? 'Unknown',
      },
    };
  });

  const graphEdges: GraphEdge[] = (graph.edges ?? []).flatMap((edge) => {
    const source = nodeKeyById.get(edge.src);
    const target = nodeKeyById.get(edge.dst);
    if (!source || !target) {
      return [];
    }

    const srcNode = nodeById.get(edge.src);
    const dstNode = nodeById.get(edge.dst);
    // For conditional structures, the branch group already owns its arm headers via the parent
    // pointer. Emitting an edge from the group to its direct child duplicates that relationship
    // and causes the UI confusion the user reported. Skip only those edges while leaving
    // everything else (e.g., sequential headers) untouched.
    if (
      srcNode?.node_type === WasmControlFlowNodeType.BranchGroup &&
      dstNode?.parent_id === edge.src
    ) {
      return [];
    }

    const graphEdge: GraphEdge = {
      id: `${source}->${target}`,
      source,
      target,
    };

    const label = dstNode?.label;
    if (label) {
      graphEdge.label = label;
    }

    return [graphEdge];
  });

  return {
    callGraph,
    callGraphDepth,
    nodes: graphNodes,
    edges: graphEdges,
    rootType: normalizedRootType,
  };
}

function mapNodeTypeToCallGraphType(
  nodeType: WasmControlFlowNodeType,
  rootType: FunctionMetadata['type']
): CallGraphNode['type'] {
  if (nodeType === WasmControlFlowNodeType.FunctionRoot) {
    if (rootType === 'llm_function') return 'llm_function';
    if (rootType === 'workflow') return 'block';
    return 'function';
  }
  return 'block';
}

function mapNodeTypeToGraphNodeType(
  nodeType: WasmControlFlowNodeType,
  rootType: FunctionMetadata['type']
): GraphNode['type'] {
  if (nodeType === WasmControlFlowNodeType.FunctionRoot) {
    if (rootType === 'llm_function') return 'llm_function';
    if (rootType === 'workflow') return 'group';
    return 'function';
  }
  switch (nodeType) {
    case WasmControlFlowNodeType.Loop:
      return 'loop';
    case WasmControlFlowNodeType.BranchGroup:
      return 'conditional';
    case WasmControlFlowNodeType.BranchArm:
    case WasmControlFlowNodeType.HeaderContextEnter:
    case WasmControlFlowNodeType.OtherScope:
    default:
      return 'group';
  }
}

function mapNodeTypeToBlockType(nodeType: WasmControlFlowNodeType): BlockType | undefined {
  switch (nodeType) {
    case WasmControlFlowNodeType.BranchGroup:
      return 'if';
    case WasmControlFlowNodeType.Loop:
      return 'loop';
    case WasmControlFlowNodeType.BranchArm:
    case WasmControlFlowNodeType.HeaderContextEnter:
    case WasmControlFlowNodeType.OtherScope:
      return 'expression';
    default:
      return undefined;
  }
}

function deriveNodeKey(node: WasmControlFlowNode, rootName: string): string {
  const lexical = node.lexical_id?.trim();
  if (lexical) {
    return lexical;
  }
  return `${rootName}_${node.id}`;
}

function computeCallGraphDepth(node: CallGraphNode | undefined): number {
  if (!node) {
    return 0;
  }
  if (!node.children || node.children.length === 0) {
    return 1;
  }
  const childDepths = node.children.map((child) => computeCallGraphDepth(child));
  return 1 + Math.max(...childDepths);
}

/**
 * Real BAML Runtime wrapping WASM
 */
export class BamlRuntime implements BamlRuntimeInterface {
  private wasmProject: WasmProject;
  private wasmRuntime: WasmRuntime | undefined;
  private diagnostics: DiagnosticError[] = [];
  private wasm: BamlWasmModule;
  private adapter: WasmTypeAdapter;

  private constructor(
    wasm: BamlWasmModule,
    wasmProject: WasmProject,
    wasmRuntime: WasmRuntime | undefined,
    diagnostics: DiagnosticError[]
  ) {
    this.wasm = wasm;
    this.wasmProject = wasmProject;
    this.wasmRuntime = wasmRuntime;
    this.diagnostics = diagnostics;
    this.adapter = new WasmTypeAdapter(wasm);
  }

  /**
   * Factory method to create a new runtime instance
   *
   * @param files - BAML files (must end with .baml)
   * @param envVars - Environment variables for runtime
   * @param featureFlags - Feature flags for runtime
   */
  static async create(
    files: Record<string, string>,
    envVars: Record<string, string> = {},
    featureFlags: string[] = []
  ): Promise<{ wasm: typeof import('@gloo-ai/baml-schema-wasm-web/baml_schema_build'), runtime: BamlRuntime }> {
    console.log('[BamlRuntime] Creating runtime with', Object.keys(files).length, 'files');

    // Get cached WASM module (loads once, then reuses)
    const wasm = await getWasmModule();

    // Filter to .baml files only
    const bamlFiles = Object.entries(files).filter(([path]) => path.endsWith('.baml'));
    console.log('[BamlRuntime] Filtered to', bamlFiles.length, 'BAML files');

    // Create WasmProject (matches wasmAtom pattern)
    const wasmProject = wasm.WasmProject.new('./', bamlFiles);

    // Try to create runtime and collect diagnostics
    let wasmRuntime: WasmRuntime | undefined;
    let diagnostics: DiagnosticError[] = [];

    try {
      console.log('[BamlRuntime] Creating runtime with env vars and feature flags', { envVars, featureFlags });
      // Create runtime with env vars and feature 
      // flags
      wasmRuntime = wasmProject.runtime(envVars, featureFlags);

      // Get diagnostics from project
      const diags = wasmProject.diagnostics(wasmRuntime);
      if (diags) {
        diagnostics = diags.errors().map((e: WasmError, index: number) => ({
          id: `diag_${index}`,
          type: e.type as 'error' | 'warning',
          message: e.message || String(e),
          filePath: e.file_path,
          line: e.start_line,
          column: e.start_column,
        }));
      }

      console.log('[BamlRuntime] Runtime created successfully with', diagnostics.length, 'diagnostics');
    } catch (e) {
      console.error('[BamlRuntime] Error creating runtime:', e);

      // Check if it's a WasmDiagnosticError
      if (wasm.WasmDiagnosticError && e instanceof wasm.WasmDiagnosticError) {
        const wasmDiagError = e as WasmDiagnosticError;
        diagnostics = wasmDiagError.errors().map((err: WasmError, index: number) => ({
          id: `diag_${index}`,
          type: err.type as 'error' | 'warning',
          message: err.message || String(err),
          filePath: err.file_path,
          line: err.start_line,
          column: err.start_column,
        }));
        console.log('[BamlRuntime] Captured', diagnostics.length, 'diagnostics from error');
      } else {
        // Unknown error - create a generic diagnostic
        diagnostics = [{
          id: 'diag_unknown',
          type: 'error',
          message: e instanceof Error ? e.message : String(e),
        }];
      }

      // Runtime is undefined if there was an error
      wasmRuntime = undefined;
    }

    return { wasm, runtime: new BamlRuntime(wasm, wasmProject, wasmRuntime, diagnostics) };
  }

  // ============================================================================
  // BamlRuntimeInterface Implementation
  // ============================================================================

  getVersion(): string {
    return this.wasm.version();
  }

  getWasmRuntime(): WasmRuntime | undefined {
    return this.wasmRuntime;
  }

  getWorkflows(): FunctionWithCallGraph[] {
    // Workflows are just root functions with call graphs
    // For now, return all functions (naive implementation)
    // TODO: Filter by isRoot: true when we properly analyze call relationships
    return this.getFunctions();
  }

  getCallGraph(functionName: string): CallGraphNode | undefined {
    const functions = this.getFunctions();
    const func = functions.find(f => f.name === functionName);
    return func?.callGraph;
  }

  getFunctions(): FunctionWithCallGraph[] {
    if (!this.wasmRuntime) {
      console.log('[BamlRuntime] Cannot get functions - runtime is invalid');
      return [];
    }

    try {
      const wasmFunctions = this.wasmRuntime.list_functions();
      const seen = new Set<string>();
      const combined: FunctionWithCallGraph[] = [];

      const pushFn = (fn: RichWasmFunction, metadata: FunctionMetadata) => {
        if (seen.has(fn.name)) {
          return;
        }
        combined.push(this.buildFunctionRecord(fn, metadata));
        seen.add(fn.name);
      };

      for (const fn of wasmFunctions) {
        const metadata =
          fn.function_type === WasmFunctionKind.Llm
            ? this.adapter.convertFunction(fn, this.wasmRuntime!)
            : this.adapter.convertExprFunction(fn);
        pushFn(fn, metadata);
      }

      return combined;
    } catch (e) {
      console.error('[BamlRuntime] Error getting functions:', e);
      return [];
    }
  }

  private buildFunctionRecord(
    fn: WasmFunction,
    metadata: FunctionMetadata
  ): FunctionWithCallGraph {
    const timestamp = Date.now();
    let controlFlow = createFallbackControlFlowArtifacts(metadata, timestamp);
    try {
      const wasmGraph = fn.function_graph_v2(this.wasmRuntime!);
      const converted = buildControlFlowArtifacts(wasmGraph, this.adapter, {
        rootName: fn.name,
        rootType: metadata.type,
        llmClient: metadata.clientName,
        timestamp,
      });
      if (converted) {
        controlFlow = converted;
      }
    } catch (graphErr) {
      console.warn(
        `[BamlRuntime] Failed to build control flow graph for ${fn.name}`,
        graphErr
      );
    }

    const resolvedSpan = (() => {
      if (metadata.span) {
        return metadata.span;
      }
      throw new Error(`[BamlRuntime] Missing span information for ${fn.name}`);
    })();

    let finalType: FunctionMetadata['type'] = controlFlow.rootType === 'workflow'
      ? 'workflow'
      : metadata.type;

    // if (finalType === 'workflow' && controlFlow.nodes.length <= 1) {
    //   // HACK: Treat single-node “workflows” (pure LLM calls) as llm_functions until
    //   // the runtime can return richer structure for them.
    //   finalType = 'llm_function';
    // }

    // if (finalType !== 'workflow') {
    //   controlFlow = createFallbackControlFlowArtifacts(
    //     { ...metadata, type: finalType },
    //     timestamp
    //   );
    // }

    return {
      ...metadata,
      type: finalType,
      span: resolvedSpan,
      callGraph: controlFlow.callGraph,
      isRoot: finalType === 'workflow' ? true : controlFlow.callGraphDepth === 1,
      callGraphDepth: controlFlow.callGraphDepth,

      // Backward compatibility fields
      id: fn.name,
      displayName: fn.name,
      filePath: resolvedSpan.filePath,
      startLine: resolvedSpan.startLine,
      endLine: resolvedSpan.endLine,
      nodes: controlFlow.nodes,
      edges: controlFlow.edges,
      entryPoint: fn.name,
      parameters: [],
      returnType: '',
      childFunctions: [],
      lastModified: timestamp,
      codeHash: '',
    };
  }

  getTestCases(functionName?: string): TestCaseMetadata[] {
    // Need valid runtime to get test cases
    if (!this.wasmRuntime) {
      console.log('[BamlRuntime] Cannot get test cases - runtime is invalid');
      return [];
    }

    try {
      // Get all test cases from WASM runtime
      const allTestCases: WasmTestCase[] = this.wasmRuntime.list_testcases();

      return allTestCases
        .filter((tc) => {
          if (!functionName) return true;
          // Filter by functionName - check if this test belongs to the specified function
          return tc.parent_functions.some((pf) => pf.name === functionName);
        })
        .map((tc) => this.adapter.convertTestCase(tc));
    } catch (e) {
      console.error('[BamlRuntime] Error getting test cases:', e);
      return [];
    }
  }

  getBAMLFiles(): BAMLFile[] {
    if (!this.wasmRuntime) {
      console.log('[BamlRuntime] Cannot get BAML files - runtime is invalid');
      return [];
    }

    try {
      const functions: FunctionWithCallGraph[] = this.getFunctions();
      const testCases: WasmTestCase[] = this.wasmRuntime.list_testcases();

      const fileMap = new Map<string, { functions: FunctionWithCallGraph[], tests: BAMLTest[] }>();
      const functionTypeByName = new Map(functions.map(fn => [fn.name, fn.type]));

      for (const fn of functions) {
        if (!fn.span) {
          console.warn('[BamlRuntime] Missing span for function while grouping files:', fn.name);
        }
        const filePath = fn.span?.filePath || 'unknown.baml';
        if (!fileMap.has(filePath)) {
          fileMap.set(filePath, { functions: [], tests: [] });
        }
        fileMap.get(filePath)!.functions.push(fn);
      }

      for (const tc of testCases) {
        const filePath = tc.span?.file_path || 'unknown.baml';
        if (!fileMap.has(filePath)) {
          fileMap.set(filePath, { functions: [], tests: [] });
        }
        const parentFn = tc.parent_functions[0];
        const parentName = parentFn?.name || 'unknown';
        const parentType = functionTypeByName.get(parentName) ?? 'function';
        const nodeType: 'llm_function' | 'function' = parentType === 'llm_function' ? 'llm_function' : 'function';

        const bamlTest: BAMLTest = {
          name: tc.name,
          functionName: parentName,
          filePath,
          nodeType,
        };

        fileMap.get(filePath)!.tests.push(bamlTest);
      }

      // Convert map to array of BAMLFile objects
      return Array.from(fileMap.entries()).map(([path, data]) => ({
        path,
        functions: data.functions,
        tests: data.tests,
      }));
    } catch (e) {
      console.error('[BamlRuntime] Error getting BAML files:', e);
      return [];
    }
  }

  getDiagnostics(): DiagnosticError[] {
    return this.diagnostics;
  }

  getGeneratedFiles(): GeneratedFile[] {
    // Only return generated files if runtime is valid
    if (!this.wasmRuntime) {
      console.log('[BamlRuntime] Cannot generate files - runtime is invalid');
      return [];
    }

    try {
      const generators: WasmGeneratorOutput[] = this.wasmProject.run_generators();
      const files = generators.flatMap((gen) =>
        gen.files.map((f) => ({
          path: f.path_in_output_dir,
          content: f.contents,
          outputDir: gen.output_dir,
        }))
      );

      console.log('[BamlRuntime] Generated', files.length, 'files');
      return files;
    } catch (e) {
      console.error('[BamlRuntime] Error generating files:', e);
      return [];
    }
  }

  async *executeWorkflow(
    workflowId: string,
    inputs: Record<string, any>,
    options?: ExecutionOptions
  ): AsyncGenerator<RichExecutionEvent> {
    // TODO: Implement workflow execution
    console.warn('[BamlRuntime] executeWorkflow() not yet implemented');
    throw new Error('Workflow execution not yet implemented for BamlRuntime');
  }

  async *executeTest(
    functionName: string,
    testName: string,
    context: TestExecutionContext
  ): AsyncGenerator<RichExecutionEvent> {
    if (!this.wasmRuntime) {
      throw new Error('Cannot execute test - runtime is invalid');
    }

    // Generate execution ID
    const executionId = `exec_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`;

    // Find the test case
    const testCases: WasmTestCase[] = this.wasmRuntime.list_testcases();
    const testCase = testCases.find((tc) => tc.name === testName);

    if (!testCase) {
      throw new Error(`Test case not found: ${testName}`);
    }

    // Get the function for this test
    const functions: WasmFunction[] = this.wasmRuntime.list_functions();
    const wasmFunction = functions.find((fn) => fn.name === functionName);

    if (!wasmFunction) {
      throw new Error(`Function not found: ${functionName}`);
    }

    const nodeId = functionName;

    try {
      // Extract inputs from test case
      const inputs: Record<string, any> = {};
      for (const param of testCase.inputs) {
        if (param.value !== undefined) {
          try {
            inputs[param.name] = JSON.parse(param.value);
          } catch {
            inputs[param.name] = param.value;
          }
        }
      }

      // Yield node enter event
      yield {
        type: 'node.enter',
        nodeId,
        timestamp: Date.now(),
        iteration: 0,
        executionId,
        inputs,
      };

      const startTime = performance.now();

      // Create a generator-friendly way to yield events from callbacks
      const events: RichExecutionEvent[] = [];
      const pushEvent = (event: RichExecutionEvent) => {
        events.push(event);
      };

      // Execute the test with all callbacks yielding events
      const result = await wasmFunction.run_test_with_expr_events(
        this.wasmRuntime,
        testCase.name,
        // on_partial_response callback
        (partial: any) => {
          pushEvent({
            type: 'partial.response',
            nodeId,
            timestamp: Date.now(),
            iteration: 0,
            executionId,
            partialContent: String(partial),
            isFinal: false,
          });
        },
        // get_baml_src_cb - load media files
        context?.loadMediaFile || vscode.loadMediaFile,
        // on_expr_event - expression evaluation events (for highlighting)
        (spans: WasmSpan[]) => {
          if (spans && spans.length > 0) {
            pushEvent({
              type: 'highlight',
              nodeId,
              timestamp: Date.now(),
              iteration: 0,
              executionId,
              spans: spans.map((span) => this.adapter.convertSpan(span)),
            });
          }
        },
        // env - API keys / environment
        context?.apiKeys || {},
        // abort_signal
        context?.abortSignal || null,
        // watch_handler - for watch notifications
        (notification: any) => {
          pushEvent({
            type: 'watch.notification',
            nodeId,
            timestamp: Date.now(),
            iteration: 0,
            executionId,
            notification: {
              variableName: notification.variable_name,
              channelName: notification.channel_name,
              blockName: notification.block_name,
              isStream: notification.is_stream,
              value: notification.value,
            },
          });
        }
      );

      // Yield all accumulated events from callbacks
      for (const event of events) {
        yield event;
      }

      const endTime = performance.now();
      const duration = endTime - startTime;

      // Parse the result
      const status = result.status();
      const statusMap = {
        [this.wasm.TestStatus.Passed]: 'passed',
        [this.wasm.TestStatus.LLMFailure]: 'llm_failed',
        [this.wasm.TestStatus.ParseFailure]: 'parse_failed',
        [this.wasm.TestStatus.ConstraintsFailed]: 'constraints_failed',
        [this.wasm.TestStatus.AssertFailed]: 'assert_failed',
        [this.wasm.TestStatus.UnableToRun]: 'error',
        [this.wasm.TestStatus.FinishReasonFailed]: 'error',
      } as const;

      const testStatus = statusMap[status] || 'error';

      // Convert WASM response to plain object
      const responseData = this.adapter.convertResponseToData(result);

      // Extract outputs
      let outputs: Record<string, any> = {};

      if (testStatus === 'passed') {
        const parsedResponse = result.parsed_response();
        if (parsedResponse) {
          try {
            outputs = { result: JSON.parse(parsedResponse.value) };
          } catch {
            outputs = { result: parsedResponse.value };
          }
        }
      } else {
        // Get error information
        const failureMsg = result.failure_message();
        if (failureMsg) {
          outputs = { error: failureMsg };
        }
      }

      // Yield completion or error event
      if (testStatus === 'passed') {
        yield {
          type: 'node.exit',
          nodeId,
          timestamp: Date.now(),
          iteration: 0,
          executionId,
          outputs,
          duration,
          responseData,
        };
      } else {
        yield {
          type: 'node.exit',
          nodeId,
          timestamp: Date.now(),
          iteration: 0,
          executionId,
          outputs,
          duration,
          responseData,
          error: {
            message: outputs.error || `Test failed with status: ${testStatus}`,
            code: testStatus,
          },
        };
      }
    } catch (error) {
      yield {
        type: 'node.exit',
        nodeId,
        timestamp: Date.now(),
        iteration: 0,
        executionId,
        duration: 0,
        error: {
          message: error instanceof Error ? error.message : String(error),
          stack: error instanceof Error ? error.stack : undefined,
        },
      };
    }
  }

  async *executeTests(
    tests: Array<{ functionName: string; testName: string }>,
    context: TestExecutionContext
  ): AsyncGenerator<RichExecutionEvent> {
    if (!this.wasmRuntime) {
      throw new Error('Cannot execute tests - runtime is invalid');
    }

    try {
      // Prepare test cases for run_tests
      const testCases = tests.map((test) => {
        const allTestCases: WasmTestCase[] = this.wasmRuntime!.list_testcases();
        const testCase = allTestCases.find((tc) => tc.name === test.testName);

        if (!testCase) {
          throw new Error(`Test case not found: ${test.testName}`);
        }

        // Convert inputs
        const inputs: Record<string, unknown> = {};
        for (const param of testCase.inputs) {
          if (param.value !== undefined) {
            try {
              inputs[param.name] = JSON.parse(param.value);
            } catch {
              inputs[param.name] = param.value;
            }
          }
        }

        return {
          functionName: test.functionName,
          testName: test.testName,
          inputs,
        };
      });

      const executionId = `batch_${Date.now()}`;

      // Yield started events for all tests
      for (const test of tests) {
        const testCase = testCases.find((tc) => tc.testName === test.testName);
        if (testCase) {
          yield {
            type: 'node.enter',
            nodeId: test.functionName,
            timestamp: Date.now(),
            iteration: 0,
            executionId,
            inputs: testCase.inputs,
          };
        }
      }

      // Create event collectors for callbacks
      const events: RichExecutionEvent[] = [];
      const pushEvent = (event: RichExecutionEvent) => {
        events.push(event);
      };

      // Execute all tests in parallel via run_tests
      const results = await this.wasmRuntime.run_tests(
        testCases,
        // on_partial_response callback
        (partial: WasmPartialResponse & { func_test_pair: () => { function_name: string; test_name: string } }) => {
          const pair = partial.func_test_pair();
          if (context.onPartialResponse) {
            context.onPartialResponse(partial);
          }
          pushEvent({
            type: 'partial.response',
            nodeId: pair.function_name,
            timestamp: Date.now(),
            iteration: 0,
            executionId,
            partialContent: String(partial),
            isFinal: false,
          });
        },
        // get_baml_src_cb - load media files
        context.loadMediaFile || vscode.loadMediaFile,
        // env - API keys / environment
        context.apiKeys || {},
        // abort_signal
        context.abortSignal || null,
        // watch_handler - for watch notifications
        (notification: WasmNotification & { function_name?: string; test_name?: string }) => {
          // Watch notifications should have function_name and test_name from parallel execution
          const watchNotification = {
            variableName: notification.variable_name,
            channelName: notification.channel_name,
            blockName: notification.block_name,
            isStream: notification.is_stream,
            value: notification.value,
          };
          if (context.onWatchNotification) {
            context.onWatchNotification(watchNotification);
          }
          pushEvent({
            type: 'watch.notification',
            nodeId: notification.function_name || 'unknown',
            timestamp: Date.now(),
            iteration: 0,
            executionId,
            notification: watchNotification,
          });
        }
      );

      // Yield all accumulated events from callbacks
      for (const event of events) {
        yield event;
      }

      // Process results
      let response: WasmTestResponse | undefined;
      while ((response = results.yield_next()) !== undefined) {
        const pair = response.func_test_pair();
        const status = response.status();

        const statusMap = {
          [this.wasm.TestStatus.Passed]: 'passed',
          [this.wasm.TestStatus.LLMFailure]: 'llm_failed',
          [this.wasm.TestStatus.ParseFailure]: 'parse_failed',
          [this.wasm.TestStatus.ConstraintsFailed]: 'constraints_failed',
          [this.wasm.TestStatus.AssertFailed]: 'assert_failed',
          [this.wasm.TestStatus.UnableToRun]: 'error',
          [this.wasm.TestStatus.FinishReasonFailed]: 'error',
        } as const;

        const testStatus = statusMap[status] || 'error';

        // Convert WASM response to plain object
        const responseData = this.adapter.convertResponseToData(response);

        // Extract outputs
        let outputs: Record<string, any> = {};
        if (testStatus === 'passed') {
          const parsedResponse = response.parsed_response();
          if (parsedResponse) {
            try {
              outputs = { result: JSON.parse(parsedResponse.value) };
            } catch {
              outputs = { result: parsedResponse.value };
            }
          }
        } else {
          const failureMsg = response.failure_message();
          if (failureMsg) {
            outputs = { error: failureMsg };
          }
        }

        // Yield completion or error event
        if (testStatus === 'passed') {
          yield {
            type: 'node.exit',
            nodeId: pair.function_name,
            timestamp: Date.now(),
            iteration: 0,
            executionId,
            outputs,
            duration: 0, // TODO: Track duration for parallel tests
            responseData,
          };
        } else {
          yield {
            type: 'node.exit',
            nodeId: pair.function_name,
            timestamp: Date.now(),
            iteration: 0,
            executionId,
            outputs,
            duration: 0,
            responseData,
            error: {
              message: outputs.error || `Test failed with status: ${testStatus}`,
              code: testStatus,
            },
          };
        }
      }
    } catch (error) {
      // Yield error for all tests
      const executionId = `batch_${Date.now()}`;
      for (const test of tests) {
        yield {
          type: 'node.exit',
          nodeId: test.functionName,
          timestamp: Date.now(),
          iteration: 0,
          executionId,
          duration: 0,
          error: {
            message: error instanceof Error ? error.message : String(error),
            stack: error instanceof Error ? error.stack : undefined,
          },
        };
      }
    }
  }

  async cancelExecution(executionId: string): Promise<void> {
    // TODO: Implement execution cancellation
    console.warn('[BamlRuntime] cancelExecution() not yet implemented');
  }

  async renderPromptForTest(
    functionName: string,
    testName: string,
    context: TestExecutionContext
  ): Promise<PromptInfo> {
    if (!this.wasmRuntime) {
      throw new Error('Runtime not initialized');
    }
    try {

      const wasmFunctions = this.wasmRuntime.list_functions();
      const wasmFn = wasmFunctions.find(f => f.name === functionName);
      if (!wasmFn) {
        throw new Error(`Function ${functionName} not found`);
      }
      if (wasmFn.function_type !== WasmFunctionKind.Llm) {
        throw new Error(`Function ${functionName} is not an LLM function`);
      }

      const wasmCallContext = new this.wasm.WasmCallContext();
      const wasmPrompt = await wasmFn.render_prompt_for_test(
        this.wasmRuntime,
        testName,
        wasmCallContext,
        context.loadMediaFile || (() => Promise.resolve('')),
        context.apiKeys || {}
      );

      // Convert WASM prompt to unified type
      return this.adapter.convertPrompt(wasmPrompt);
    } catch (error) {
      console.error('[BamlRuntime] Error rendering prompt for test:', error);
      throw error;
    }
  }

  async renderCurlForTest(
    functionName: string,
    testName: string,
    options: {
      stream: boolean;
      expandImages: boolean;
      exposeSecrets: boolean;
    },
    context: TestExecutionContext
  ): Promise<string> {
    try {
      if (!this.wasmRuntime) {
        throw new Error('Runtime not initialized');
      }

      const wasmFunctions = this.wasmRuntime.list_functions();
      const wasmFn = wasmFunctions.find(f => f.name === functionName);
      if (!wasmFn) {
        throw new Error(`Function ${functionName} not found`);
      }
      if (wasmFn.function_type !== WasmFunctionKind.Llm) {
        throw new Error(`Function ${functionName} is not an LLM function`);
      }

      const wasmCallContext = new this.wasm.WasmCallContext();
      return await wasmFn.render_raw_curl_for_test(
        this.wasmRuntime,
        testName,
        wasmCallContext,
        options.stream || false,
        options.expandImages || false,
        context.loadMediaFile || (() => Promise.resolve('')),
        context.apiKeys || {},
        options.exposeSecrets || false
      );
    } catch (error) {
      console.error('[BamlRuntime] Error rendering curl for test:', error);
      throw error;
    }
  }

  updateCursor(
    cursor: CursorPosition,
    fileContents: Record<string, string>,
    currentSelection: string | null
  ): CursorNavigationResult {
    if (!this.wasmRuntime) {
      console.log('no wasm runtime');
      return { functionName: null, testCaseName: null, nodeId: null };
    }
    try {

      const fileContent = fileContents[cursor.fileName];
      if (!fileContent) {
        console.log('no file content');
        return { functionName: null, testCaseName: null, nodeId: null };
      }

      // Convert line/column to character index
      const lines = fileContent.split('\n');
      let cursorIdx = 0;
      for (let i = 0; i < cursor.line; i++) {
        cursorIdx += (lines[i]?.length ?? 0) + 1; // +1 for newline
      }
      cursorIdx += cursor.column;

      const entity = this.wasmRuntime.get_entity_at_position(
        cursor.fileName,
        cursorIdx
      );
      if (entity) {
        console.log('[BamlRuntime] Entity at cursor:', {
          entity_type: entity.entity_type,
          entity_name: entity.entity_name,
          node_id: entity.node_id,
          node_label: entity.node_label,
        });
      }

      // Get function at cursor position
      const selectedFunc = this.wasmRuntime.get_function_at_position(
        cursor.fileName,
        currentSelection ?? '',
        cursorIdx
      );

      if (!selectedFunc) {
        console.log('aaron: clicked on something that is not a function');
        return { functionName: null, testCaseName: null, nodeId: null };
      }

      // IMPORTANT, if we dont extract the name here, when we try doing selectedFunc.name after passing selectedFunc to get_testcase_from_position, it will throw a 'passed null pointer to rust' error.
      const name = selectedFunc.name;

      // Check if cursor is in a test case
      const selectedTestcase = this.wasmRuntime.get_testcase_from_position(
        selectedFunc,
        cursorIdx
      );


      if (selectedTestcase) {
        // Check for nested function in test case
        const nestedFunc = this.wasmRuntime.get_function_of_testcase(
          cursor.fileName,
          cursorIdx
        );


        return {
          functionName: nestedFunc ? nestedFunc.name : name,
          testCaseName: selectedTestcase.name,
          nodeId: null,
        };
      }
      // selectedFunc.name; // uncomment to repro the 'null pointer passed to rust error'.

      // Extract node_id from entity if available (for workflow nodes)
      const nodeId = entity?.node_id ?? null;

      console.log('[BamlRuntime] updateCursor returning:', {
        functionName: name,
        nodeId,
        entity_type: entity?.entity_type,
        entity_name: entity?.entity_name,
        node_label: entity?.node_label,
      });

      // Just a function, no test case
      return {
        functionName: name,
        testCaseName: null,
        nodeId,
      };
    } catch (error) {
      console.error('[BamlRuntime] Error updating cursor:', error);
      throw error;
    }
  }

  getEntityAtPosition(
    cursor: CursorPosition,
    fileContents: Record<string, string>
  ): EntityAtPosition | null {
    if (!this.wasmRuntime) {
      return null;
    }
    try {

      const fileContent = fileContents[cursor.fileName];
      if (!fileContent) {
        return null;
      }

      // Convert line/column to character index
      const lines = fileContent.split('\n');
      let cursorIdx = 0;
      for (let i = 0; i < cursor.line; i++) {
        cursorIdx += (lines[i]?.length ?? 0) + 1; // +1 for newline
      }
      cursorIdx += cursor.column;

      // Get entity at cursor position from WASM
      const entity = this.wasmRuntime.get_entity_at_position(
        cursor.fileName,
        cursorIdx
      );

      if (!entity) {
        return null;


      }


      const wasmKinds = this.wasm?.WasmFunctionKind;
      let functionType: 'Llm' | 'Expr' | undefined;
      if (wasmKinds != null) {
        if (entity.function_type === wasmKinds.Llm) {
          functionType = 'Llm';
        } else if (entity.function_type === wasmKinds.Expr) {
          functionType = 'Expr';
        }
      }

      return {
        entity_type: entity.entity_type as 'function' | 'node',
        entity_name: entity.entity_name,
        function_type: functionType,
        node_id: entity.node_id ?? undefined,
        node_label: entity.node_label ?? undefined,
        span: entity.span,
      };
    } catch (error) {
      console.error('[BamlRuntime] Error getting entity at position:', error);
      throw error;
    }
  }
}
