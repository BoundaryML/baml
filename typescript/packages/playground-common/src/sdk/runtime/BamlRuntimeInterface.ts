/**
 * BAML Runtime Interface
 *
 * Responsible for:
 * - Loading and parsing BAML files (pushed from external sources)
 * - Discovering workflows, functions, and tests
 * - Executing workflows and tests (stateless)
 *
 * The runtime is PASSIVE and IMMUTABLE:
 * - Doesn't watch files - files are pushed to it
 * - Once created, runtime is immutable
 * - On file changes, create a new runtime instance (like wasmAtom pattern)
 *
 * Key philosophy:
 * - Everything is a function (no special "workflow" execution)
 * - getWorkflows() returns root functions with call graphs
 * - executeTest() works for any function (workflow or standalone)
 * - Events have node IDs, timestamps, iterations for graph mapping
 */

import type {
  TestCaseInput,
  BAMLFile,
  LogEntry,
} from '../types';

import type { DiagnosticError, GeneratedFile } from '../atoms/core.atoms';
import type { WasmRuntime } from '@gloo-ai/baml-schema-wasm-web';

// Import unified types from interface layer
import type {
  FunctionMetadata,
  FunctionWithCallGraph,
  TestCaseMetadata,
  CallGraphNode,
  PromptInfo,
  RichExecutionEvent,
  TestExecutionContext,
} from '../interface';

/**
 * Cursor position in a file
 */
export interface CursorPosition {
  fileName: string;
  line: number;
  column: number;
}

/**
 * Result of cursor navigation - which function/test is at the cursor
 */
export interface CursorNavigationResult {
  functionName: string | null;
  testCaseName: string | null;
  nodeId?: string | null;
}

export interface ExecutionOptions {
  clearCache?: boolean;
  startFromNodeId?: string;
}

/**
 * BAML Runtime Interface
 * Works with unified types only - no WASM dependencies
 *
 * Key philosophy:
 * - Everything is a function (no special "workflow" execution)
 * - getWorkflows() returns root functions with call graphs
 * - executeTest() works for any function (workflow or standalone)
 * - Events have node IDs, timestamps, iterations for graph mapping
 */
export interface BamlRuntimeInterface {
  // ============================================================================
  // METADATA
  // ============================================================================

  /**
   * Get BAML runtime version
   */
  getVersion(): string;

  /**
   * Get the WASM runtime instance (for legacy compatibility with wasmAtom)
   * This returns the WasmRuntime, not the WASM module
   * Returns undefined for mock runtimes or when runtime has errors
   */
  getWasmRuntime(): WasmRuntime | undefined;

  // ============================================================================
  // FUNCTIONS & CALL GRAPHS
  // ============================================================================

  /**
   * Get all functions with their call graphs
   *
   * Returns all functions, including their static call graphs.
   * The UI can filter for root functions (isRoot: true) to show as "workflows"
   */
  getFunctions(): FunctionWithCallGraph[];

  /**
   * Get workflows (root functions with non-trivial call graphs)
   *
   * This is essentially a filter over getFunctions():
   * - Returns only root functions (not called by others)
   * - Could naively return all functions
   * - Could filter by call graph depth > 1
   *
   * The UI treats these as "workflows" but the runtime just sees functions.
   */
  getWorkflows(): FunctionWithCallGraph[];

  /**
   * Get call graph for a specific function
   */
  getCallGraph(functionName: string): CallGraphNode | undefined;

  // ============================================================================
  // TEST CASES
  // ============================================================================

  /**
   * Get test cases (unified type, not WASM type)
   */
  getTestCases(functionName?: string): TestCaseMetadata[];

  // ============================================================================
  // FILES & DIAGNOSTICS
  // ============================================================================

  /**
   * Get BAML file structure (for debug panel)
   */
  getBAMLFiles(): BAMLFile[];

  /**
   * Get compilation diagnostics (errors and warnings)
   * Returns empty array if compilation was successful
   */
  getDiagnostics(): DiagnosticError[];

  /**
   * Get generated code files from BAML runtime
   * Returns empty array if no generators are configured or runtime is invalid
   */
  getGeneratedFiles(): GeneratedFile[];

  // ============================================================================
  // EXECUTION (Rich Events)
  // ============================================================================

  /**
   * Execute a workflow (stateless - just yields events)
   * Does NOT track state - that's the SDK's job
   * @deprecated Workflows are just functions - use executeTest instead
   */
  executeWorkflow(
    workflowId: string,
    inputs: Record<string, unknown>,
    options?: ExecutionOptions
  ): AsyncGenerator<RichExecutionEvent>;

  /**
   * Execute multiple tests
   *
   * All real-time updates happen through callbacks in TestExecutionContext:
   * - onPartialResponse: Called with TestResponseData as responses stream in
   * - onWatchNotification: Called when watch blocks emit notifications
   * - onHighlight: Called when code should be highlighted
   *
   * These callbacks are called synchronously by WASM during execution,
   * providing true real-time updates without generator blocking issues.
   *
   * Returns a promise that resolves when all tests complete.
   */
  executeTests(
    tests: Array<{ functionName: string; testName: string }>,
    context: TestExecutionContext
  ): Promise<void>;

  /**
   * Cancel a running execution
   */
  cancelExecution(executionId: string): Promise<void>;

  // ============================================================================
  // LLM-SPECIFIC OPERATIONS
  // ============================================================================

  /**
   * Render prompt for a test case
   * Returns actual prompt that would be sent to LLM
   */
  renderPromptForTest(
    functionName: string,
    testName: string,
    context: TestExecutionContext
  ): Promise<PromptInfo>;

  /**
   * Render curl command for a test case
   * Useful for debugging/testing outside BAML
   */
  renderCurlForTest(
    functionName: string,
    testName: string,
    options: {
      stream: boolean;
      expandImages: boolean;
      exposeSecrets: boolean;
    },
    context: TestExecutionContext
  ): Promise<string>;

  // ============================================================================
  // NAVIGATION
  // ============================================================================

  /**
   * Update cursor position and determine which function/test is at that position
   *
   * @param cursor - Cursor position (fileName, line, column)
   * @param fileContents - Map of file paths to their contents
   * @param currentSelection - Currently selected function name (for context)
   * @returns The function and test case at the cursor position
   */
  updateCursor(
    cursor: CursorPosition,
    fileContents: Record<string, string>,
    currentSelection: string | null
  ): CursorNavigationResult;

}

/**
 * Factory type for creating runtime instances
 * Accepts files, environment variables, and feature flags
 */
export type BamlRuntimeFactory = (
  files: Record<string, string>,
  envVars?: Record<string, string>,
  featureFlags?: string[]
) => Promise<BamlRuntimeInterface>;
