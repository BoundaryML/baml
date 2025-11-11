/**
 * Core Jotai Atoms for BAML SDK State Management
 *
 * All atoms consolidated into a single file for better organization.
 * Follows the design principle: minimize atoms, compute derived state in hooks.
 */

import { atom } from 'jotai';
import { atomFamily } from 'jotai/utils';
import type {
  ExecutionSnapshot,
  NodeExecutionState,
  CacheEntry,
  BAMLEvent,
} from '../types';
import type { BamlRuntimeInterface } from '../runtime/BamlRuntimeInterface';
import type { FunctionMetadata, FunctionWithCallGraph } from '../interface';
import type { WasmRuntime } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';

// ============================================================================
// CORE STATE (source of truth)
// ============================================================================

/**
 * Runtime instance atom - the source of truth for all derived state
 * When this changes, all derived atoms (functions, diagnostics, workflows, etc.) automatically update
 */
export const runtimeInstanceAtom = atom<BamlRuntimeInterface | null>(null);

/**
 * Unified selection state interface
 */
export interface UnifiedSelectionState {
  functionName: string | null;
  testName: string | null;
  activeWorkflowId: string | null;
  selectedNodeId: string | null;
}

/**
 * Unified selection state - SINGLE SOURCE OF TRUTH for all selection
 * All other selection atoms derive from this
 */
export const unifiedSelectionStateAtom = atom<UnifiedSelectionState>({
  functionName: null,
  testName: null,
  activeWorkflowId: null,
  selectedNodeId: null,
});

/**
 * All available workflows (derived from runtime)
 */
export const workflowsAtom = atom((get) => {
  const runtime = get(runtimeInstanceAtom);
  return runtime?.getWorkflows() ?? [];
}, (get, set, update: FunctionWithCallGraph[]) => {
  set(workflowsAtom, update);
});

/**
 * Currently active workflow ID (derived - read-only)
 * This is part of the unified selection state
 */
export const activeWorkflowIdAtom = atom((get) => get(unifiedSelectionStateAtom).activeWorkflowId);

/**
 * Executions stored per workflow using atomFamily
 * This allows granular subscriptions per workflow
 */
export const workflowExecutionsAtomFamily = atomFamily((workflowId: string) =>
  atom<ExecutionSnapshot[]>([])
);

/**
 * Selected execution ID (for viewing snapshots)
 */
export const selectedExecutionIdAtom = atom<string | null>(null);

/**
 * Node states using atomFamily for granular updates
 * Each node gets its own atom, so updates to one node don't trigger re-renders of other nodes
 */
export const nodeStateAtomFamily = atomFamily((_nodeId: string) =>
  atom<NodeExecutionState>('not-started')
);

/**
 * Registry of all node IDs that have been created
 * Used to track which nodes exist
 */
const nodeRegistryAtom = atom<Set<string>>(new Set<string>());

/**
 * Write-only atom to register a node
 */
export const registerNodeAtom = atom(
  null,
  (get, set, nodeId: string) => {
    const registry = get(nodeRegistryAtom);
    if (!registry.has(nodeId)) {
      const newRegistry = new Set(registry);
      newRegistry.add(nodeId);
      set(nodeRegistryAtom, newRegistry);
    }
  }
);

/**
 * Write-only atom to clear all node states
 */
export const clearAllNodeStatesAtom = atom(
  null,
  (get, set) => {
    const registry = get(nodeRegistryAtom);
    registry.forEach((nodeId) => {
      set(nodeStateAtomFamily(nodeId), 'not-started');
    });
  }
);

/**
 * Read-only atom to get all node states as a Map
 */
export const allNodeStatesAtom = atom((get) => {
  const registry = get(nodeRegistryAtom);
  const states = new Map<string, NodeExecutionState>();
  registry.forEach((nodeId) => {
    states.set(nodeId, get(nodeStateAtomFamily(nodeId)));
  });
  return states;
});

/**
 * Cache entries
 */
export const cacheAtom = atom<Map<string, CacheEntry>>(new Map());

/**
 * Helper to generate cache key
 */
export function getCacheKey(nodeId: string, inputsHash: string): string {
  return `${nodeId}:${inputsHash}`;
}

/**
 * Event stream (circular buffer, keep last 100 events)
 */
export const eventStreamAtom = atom<BAMLEvent[]>([]);

/**
 * Write-only atom to add an event
 */
export const addEventAtom = atom(
  null,
  (get, set, event: BAMLEvent) => {
    const events = get(eventStreamAtom);
    const newEvents = [...events, event].slice(-100); // Keep last 100
    set(eventStreamAtom, newEvents);
  }
);

// ============================================================================
// UI STATE
// ============================================================================

/**
 * View mode: editor or execution snapshot
 */
export const viewModeAtom = atom<{ mode: 'editor' | 'execution' }>({
  mode: 'editor',
});

/**
 * Selected node ID (derived - read-only)
 * This is part of the unified selection state
 */
export const selectedNodeIdAtom = atom((get) => get(unifiedSelectionStateAtom).selectedNodeId);

/**
 * Detail panel state
 */
export interface DetailPanelState {
  isOpen: boolean;
  position: 'bottom' | 'right' | 'floating';
  activeTab: 'io' | 'logs' | 'history';
}

export const detailPanelAtom = atom<DetailPanelState>({
  isOpen: false,
  position: 'bottom',
  activeTab: 'io',
});

/**
 * Layout direction for graph
 */
export const layoutDirectionAtom = atom<'vertical' | 'horizontal'>('vertical');

/**
 * Selected input source for a node
 */
export const selectedInputSourceAtom = atom<{
  nodeId: string;
  sourceType: 'execution' | 'test' | 'manual';
  sourceId: string;
} | null>(null);

/**
 * Active node inputs (editable)
 */
export const activeNodeInputsAtom = atom<Record<string, any>>({});

/**
 * Inputs dirty flag
 */
export const inputsDirtyAtom = atom<boolean>(false);

/**
 * BAML files
 */
export const bamlFilesAtom = atom<any[]>([]);

/**
 * Active code click event
 */
export const activeCodeClickAtom = atom<any | null>(null);

// ============================================================================
// DERIVED STATE (computed from core state)
// ============================================================================

/**
 * Active workflow (derived from activeWorkflowIdAtom and workflowsAtom)
 */
export const activeWorkflowAtom = atom((get) => {
  const id = get(activeWorkflowIdAtom);
  if (!id) return null;
  const workflows = get(workflowsAtom);
  return workflows.find((w) => w.id === id) || null;
});

/**
 * Active workflow executions (derived)
 */
export const activeWorkflowExecutionsAtom = atom((get) => {
  const id = get(activeWorkflowIdAtom);
  if (!id) return [];
  return get(workflowExecutionsAtomFamily(id));
});

/**
 * Selected execution (derived)
 */
export const selectedExecutionAtom = atom((get) => {
  const selectedId = get(selectedExecutionIdAtom);
  if (!selectedId) return null;

  // Search through all workflows
  const workflows = get(workflowsAtom);
  for (const workflow of workflows) {
    const executions = get(workflowExecutionsAtomFamily(workflow.id));
    const execution = executions.find((e) => e.id === selectedId);
    if (execution) return execution;
  }
  return null;
});

/**
 * Latest execution for active workflow (derived)
 */
export const latestExecutionAtom = atom((get) => {
  const executions = get(activeWorkflowExecutionsAtom);
  return executions[0] || null;
});

/**
 * Node executions from latest execution (derived)
 */
export const nodeExecutionsAtom = atom((get) => {
  const latestExecution = get(latestExecutionAtom);
  return latestExecution?.nodeExecutions || new Map();
});

/**
 * Write-only atom to select an execution
 */
export const selectExecutionAtom = atom(
  null,
  (_get, set, executionId: string | null) => {
    set(selectedExecutionIdAtom, executionId);
    if (executionId) {
      set(viewModeAtom, { mode: 'execution' });
    }
  }
);

/**
 * Recent workflows (derived - last 5)
 */
export const recentWorkflowsAtom = atom((get) => {
  const workflows = get(workflowsAtom);
  return workflows
    .slice()
    .sort((a, b) => b.lastModified - a.lastModified)
    .slice(0, 5);
});

/**
 * All functions from BAML runtime indexed by name for O(1) lookup
 *
 * This is a legitimate performance optimization - instead of looping through
 * functions every time, we build a Map once and cache it.
 * Updates automatically when functionsAtom changes.
 */
export const allFunctionsMapAtom = atom((get): Map<string, FunctionWithCallGraph> => {
  const functions = get(functionsAtom);
  const functionsMap = new Map<string, FunctionWithCallGraph>();

  for (const func of functions) {
    functionsMap.set(func.name, func);
  }

  return functionsMap;
});

// ============================================================================
// SELECTION STATE (Function & Test Case)
// ============================================================================

/**
 * Currently selected function name (derived - read-only from unified selection state)
 */
export const selectedFunctionNameAtom = atom((get) => get(unifiedSelectionStateAtom).functionName);

/**
 * Currently selected test case name (derived - read-only from unified selection state)
 */
export const selectedTestCaseNameAtom = atom((get) => get(unifiedSelectionStateAtom).testName);

/**
 * Selected function object (derived from bamlFilesAtom + selectedFunctionNameAtom)
 * Returns the full function object with all metadata
 */
export const selectedFunctionObjectAtom = atom((get): FunctionWithCallGraph | null => {
  const funcName = get(selectedFunctionNameAtom);
  if (!funcName) return null;

  const functionsMap = get(allFunctionsMapAtom);
  return functionsMap.get(funcName) || null;
});

/**
 * Selected test case object (derived)
 * Returns the test case from the selected function
 */
export const selectedTestCaseAtom = atom((get) => {
  const func = get(selectedFunctionObjectAtom);
  const tcName = get(selectedTestCaseNameAtom);
  if (!func || !tcName) return null;

  return func.testCases?.find((tc) => tc.name === tcName) || null;
});

/**
 * Combined selection atom (for backward compatibility with old code)
 * Returns { selectedFn, selectedTc }
 */
export const selectionAtom = atom((get) => ({
  selectedFn: get(selectedFunctionObjectAtom),
  selectedTc: get(selectedTestCaseAtom),
}));

/**
 * Write-only atom to update selection (shared by updateCursor and DebugPanel)
 * This is the central place where selection state is updated
 */
export const updateSelectionAtom = atom(
  null,
  (get, set, update: { functionName: string | null; testCaseName?: string | null }) => {
    console.log('[updateSelection]', update);

    const current = get(unifiedSelectionStateAtom);
    set(unifiedSelectionStateAtom, {
      ...current,
      functionName: update.functionName,
      testName: update.testCaseName ?? null,
    });
  }
);

/**
 * Function test snippet atom - generates test code template
 */
export const functionTestSnippetAtom = (functionName: string) => atom((get) => {
  const functionsMap = get(allFunctionsMapAtom);
  const func = functionsMap.get(functionName);

  if (!func) return null;

  // Generate test snippet based on function signature
  // This is a placeholder - actual implementation would generate proper test code
  return `test MyTest {
  functions [${functionName}]
  args {
    // Add your test arguments here
  }
}`;
});

// ============================================================================
// WASM PANIC HANDLING
// ============================================================================

export interface WasmPanicState {
  msg: string;
  timestamp: number;
}

/**
 * WASM panic state - tracks runtime panics
 */
export const wasmPanicAtom = atom<WasmPanicState | null>(null);

// ============================================================================
// DIAGNOSTICS SYSTEM
// ============================================================================

export interface DiagnosticError {
  id: string;
  type: 'error' | 'warning';
  message: string;
  filePath?: string;
  line?: number;
  column?: number;
  // For compatibility with old code (CodeMirror needs these)
  start_ch?: number;
  end_ch?: number;
}

/**
 * Compilation diagnostics (errors and warnings) - derived from runtime
 */
export const diagnosticsAtom = atom((get) => {
  const runtime = get(runtimeInstanceAtom);
  return runtime?.getDiagnostics() ?? [];
});

/**
 * All functions extracted from BAML runtime - derived from runtime
 */
export const functionsAtom = atom((get): FunctionWithCallGraph[] => {
  const runtime = get(runtimeInstanceAtom);
  return runtime?.getFunctions() ?? [];
});

/**
 * Whether the current runtime is valid (no compilation errors) - derived from diagnostics
 */
export const isRuntimeValid = atom((get) => {
  const diagnostics = get(diagnosticsAtom);
  return !diagnostics.some((d) => d.type === 'error');
});

/**
 * Derived atom: count of errors and warnings
 */
export const numErrorsAtom = atom((get) => {
  const errors = get(diagnosticsAtom);
  const warningCount = errors.filter((e) => e.type === 'warning').length;
  return { errors: errors.length - warningCount, warnings: warningCount };
});

// ============================================================================
// GENERATED FILES
// ============================================================================

export interface GeneratedFile {
  path: string;
  content: string;
  outputDir: string;
}

/**
 * Generated code files from BAML runtime - derived from runtime
 */
export const generatedFilesAtom = atom((get) => {
  const runtime = get(runtimeInstanceAtom);
  return runtime?.getGeneratedFiles() ?? [];
});

/**
 * Generated files filtered by language using atomFamily
 */
export const generatedFilesByLangAtomFamily = atomFamily((lang: string) =>
  atom((get) => {
    const files = get(generatedFilesAtom);
    return files.filter((f) => f.outputDir.includes(lang));
  })
);

// ============================================================================
// FEATURE FLAGS
// ============================================================================

/**
 * Feature flags for the runtime
 */
export const featureFlagsAtom = atom<string[]>([]);

/**
 * Derived atom: whether beta features are enabled
 */
export const betaFeatureEnabledAtom = atom((get) => {
  return get(featureFlagsAtom).includes('beta');
});

// ============================================================================
// FILES TRACKING
// ============================================================================

/**
 * Current BAML files being used by the runtime
 */
export const bamlFilesTrackedAtom = atom<Record<string, string>>({});

/**
 * Sandbox files (temporary/test files)
 */
export const sandboxFilesTrackedAtom = atom<Record<string, string>>({});

// ============================================================================
// VSCODE INTEGRATION
// ============================================================================

export interface VSCodeSettings {
  enablePlaygroundProxy?: boolean;
  featureFlags?: string[];
}

/**
 * VSCode settings (async loaded)
 */
export const vscodeSettingsAtom = atom<VSCodeSettings | null>(null);

/**
 * Playground proxy port
 */
export const playgroundPortAtom = atom<number>(0);

/**
 * Derived atom: proxy URL configuration
 */
export const proxyUrlAtom = atom((get) => {
  const vscodeSettings = get(vscodeSettingsAtom);
  const port = get(playgroundPortAtom);
  const proxyUrl = port && port !== 0 ? `http://localhost:${port}` : undefined;
  const proxyEnabled = !!vscodeSettings?.enablePlaygroundProxy;
  return { proxyEnabled, proxyUrl };
});

// ============================================================================
// ENVIRONMENT VARIABLES
// ============================================================================

/**
 * Environment variables/API keys for runtime
 */
export const envVarsAtom = atom<Record<string, string>>({});

// ============================================================================
// RUNTIME & WASM STATE (for compatibility with old code)
// ============================================================================

/**
 * Orchestration graph index for WasmCallContext
 * Used when rendering prompts to track which node in the orchestration graph is being evaluated
 */
export const orchIndexAtom = atom(0);

/**
 * WASM Runtime instance - derived from runtimeInstanceAtom
 * Contains the WasmRuntime instance (recreated on file changes)
 */
export const wasmAtom = atom<typeof import('@gloo-ai/baml-schema-wasm-web/baml_schema_build') | undefined>(undefined);

/**
 * Last valid WASM Runtime instance (without errors)
 * Stores the most recent error-free WasmRuntime for fallback use
 */
const lastValidWasmAtomInternal = atom<WasmRuntime | undefined>(undefined);
export const lastValidWasmAtom = atom(
  (get) => get(lastValidWasmAtomInternal),
  (get, set, update: WasmRuntime | undefined) => {
    set(lastValidWasmAtomInternal, update);
  }
);

/**
 * Current BAML files being tracked (derived from bamlFilesTrackedAtom)
 * This ensures the atom updates when files change via sdk.files.update()
 */
export const filesAtom = atom((get) => get(bamlFilesTrackedAtom));

/**
 * Current WasmRuntime instance - derived from runtimeInstanceAtom
 * This is the actual WasmRuntime object (not the WASM module)
 */
export const currentWasmRuntimeAtom = atom((get) => {
  const runtime = get(runtimeInstanceAtom);
  return runtime?.getWasmRuntime();
});

/**
 * Runtime state with diagnostics and last valid runtime
 * Mimics the old runtimeAtom structure for backward compatibility
 */
export const runtimeAtom = atom((get) => {
  const diagnostics = get(diagnosticsAtom);
  const currentWasmRuntime = get(currentWasmRuntimeAtom);
  const lastValidWasm = get(lastValidWasmAtom);
  const hasErrors = diagnostics.some((d) => d.type === 'error');

  // Return structure compatible with old code
  // rt and lastValidRt should be WasmRuntime instances
  return {
    rt: hasErrors ? undefined : currentWasmRuntime,
    diags: diagnostics,
    lastValidRt: hasErrors && lastValidWasm ? lastValidWasm : currentWasmRuntime,
  };
});

/**
 * Call context for WASM operations
 * Used for passing context to WASM function calls
 *
 * This is a derived atom that creates a WasmCallContext from the WASM module
 * and sets the node_index from orchIndexAtom
 */
export const ctxAtom = atom((get) => {
  const wasm = get(wasmAtom);
  if (wasm === undefined) {
    return undefined;
  }
  const context = new wasm.WasmCallContext();
  const orch_index = get(orchIndexAtom);
  context.node_index = orch_index;
  return context;
});

// ============================================================================
// VERSION INFORMATION
// ============================================================================

/**
 * BAML runtime version - derived from runtime
 */
export const versionAtom = atom((get) => {
  const runtime = get(runtimeInstanceAtom);
  return runtime?.getVersion() ?? "Loading...";
});
