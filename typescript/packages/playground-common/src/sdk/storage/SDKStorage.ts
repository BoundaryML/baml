/**
 * Storage interface - abstracts state management
 *
 * This interface allows the SDK to be storage-agnostic.
 * Implementations can use Jotai, Redux, Zustand, or any other state management solution.
 */

import type {
  ExecutionSnapshot,
  NodeExecutionState,
  NodeExecution,
  CacheEntry,
} from '../types';

import type { RichExecutionEvent } from '../interface/events';

import type {
  DiagnosticError,
  GeneratedFile,
  WasmPanicState,
  VSCodeSettings,
  SelectionState,
} from '../atoms/core.atoms';

import type {
  TestHistoryRun,
  TestHistoryEntry,
  TestState,
  WatchNotification,
  FlashRange,
  PendingTestCommand,
  PendingFunctionSelection,
} from '../atoms/test.atoms';

import type { BamlRuntimeInterface } from '../runtime/BamlRuntimeInterface';
import type { FunctionWithCallGraph } from '../interface';
import type { WasmRuntime } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';
import { BamlRuntime } from '../runtime/BamlRuntime';
import type { createStore } from 'jotai';

/**
 * Storage interface for SDK state management
 */
export interface SDKStorage {
  // ============================================================================
  // Jotai Store (for navigation to access atoms directly)
  // ============================================================================

  /**
   * Raw Jotai store for direct atom access (used by navigation system)
   */
  store: ReturnType<typeof createStore>;

  // ============================================================================
  // Runtime Instance (source of truth for derived state)
  // ============================================================================

  setWasm(wasm: typeof import('@gloo-ai/baml-schema-wasm-web/baml_schema_build') | undefined): void;
  getWasm(): typeof import('@gloo-ai/baml-schema-wasm-web/baml_schema_build') | undefined;
  setRuntime(runtime: BamlRuntimeInterface | null): void;
  getRuntime(): BamlRuntimeInterface | null;

  // ============================================================================
  // Workflows
  // ============================================================================

  setWorkflows(workflows: FunctionWithCallGraph[]): void;
  getWorkflows(): FunctionWithCallGraph[];
  setActiveWorkflowId(id: string | null): void;
  getActiveWorkflowId(): string | null;

  // ============================================================================
  // Executions
  // ============================================================================

  addExecution(workflowId: string, execution: ExecutionSnapshot): void;
  getExecutions(workflowId: string): ExecutionSnapshot[];
  updateExecution(executionId: string, updates: Partial<ExecutionSnapshot>): void;

  // ============================================================================
  // Node States
  // ============================================================================

  setNodeState(nodeId: string, state: NodeExecutionState): void;
  getNodeState(nodeId: string): NodeExecutionState;
  clearAllNodeStates(): void;

  // ============================================================================
  // Node Iterations (for loops)
  // ============================================================================

  setNodeIteration(nodeId: string, iteration: number): void;
  getNodeIteration(nodeId: string): number;
  incrementNodeIteration(nodeId: string): number;
  getLoopOrdinals(): Map<string, number>;
  setLoopOrdinal(loopPath: string, ordinal: number): void;
  clearAllNodeIterations(): void;

  // ============================================================================
  // Node Executions (I/O data)
  // ============================================================================

  addNodeExecution(executionId: string, nodeId: string, data: NodeExecution): void;
  getNodeExecution(executionId: string, nodeId: string): NodeExecution | null;

  // ============================================================================
  // Cache
  // ============================================================================

  setCacheEntry(entry: CacheEntry): void;
  getCacheEntry(nodeId: string, inputsHash: string): CacheEntry | null;
  clearCache(scope?: { workflowId?: string; nodeId?: string }): void;

  // ============================================================================
  // Version
  // ============================================================================

  getVersion(): string;

  // ============================================================================
  // WASM Instance (for legacy compatibility)
  // Stores the last valid (error-free) WasmRuntime instance
  // ============================================================================

  setWasmRuntime(wasm: WasmRuntime | undefined): void;
  getWasmRuntime(): WasmRuntime | undefined;

  // ============================================================================
  // Diagnostics
  // ============================================================================

  getDiagnostics(): DiagnosticError[];

  // ============================================================================
  // Functions
  // ============================================================================

  getFunctions(): FunctionWithCallGraph[];

  // ============================================================================
  // Generated Files
  // ============================================================================

  getGeneratedFiles(): GeneratedFile[];

  // ============================================================================
  // WASM Panic
  // ============================================================================

  setWasmPanic(panic: WasmPanicState | null): void;
  getWasmPanic(): WasmPanicState | null;

  // ============================================================================
  // Feature Flags
  // ============================================================================

  setFeatureFlags(flags: string[]): void;
  getFeatureFlags(): string[];

  // ============================================================================
  // Environment Variables
  // ============================================================================

  setEnvVars(envVars: Record<string, string>): void;
  getEnvVars(): Record<string, string>;

  // ============================================================================
  // Files Tracking
  // ============================================================================

  setBAMLFiles(files: Record<string, string>): void;
  getBAMLFiles(): Record<string, string>;
  setParsedBAMLFiles(files: any[]): void;
  setSandboxFiles(files: Record<string, string>): void;
  getSandboxFiles(): Record<string, string>;

  // ============================================================================
  // VSCode Integration
  // ============================================================================

  setVSCodeSettings(settings: VSCodeSettings | null): void;
  getVSCodeSettings(): VSCodeSettings | null;
  setPlaygroundPort(port: number): void;
  getPlaygroundPort(): number;

  // ============================================================================
  // Selection State (Function & Test Case)
  // ============================================================================

  getSelectedFunctionName(): string | null;
  getSelectedTestCaseName(): string | null;
  getUnifiedSelectionState(): SelectionState;

  // ============================================================================
  // Test Execution State
  // ============================================================================

  // Test History
  addTestHistoryRun(run: TestHistoryRun): void;
  getTestHistory(): TestHistoryRun[];
  updateTestInHistoryByRunId(runId: string, testIndex: number, update: TestState): void;
  setSelectedHistoryIndex(index: number): void;
  getSelectedHistoryIndex(): number;

  // Test Execution State
  setAreTestsRunning(running: boolean): void;
  getAreTestsRunning(): boolean;
  setCurrentAbortController(controller: AbortController | null): void;
  getCurrentAbortController(): AbortController | null;

  // Watch Notifications & Highlighting
  setCurrentWatchNotifications(notifications: WatchNotification[]): void;
  getCurrentWatchNotifications(): WatchNotification[];
  addWatchNotification(notification: WatchNotification): void;
  clearWatchNotifications(): void;

  setHighlightedBlocks(blocks: Set<string>): void;
  getHighlightedBlocks(): Set<string>;
  addHighlightedBlock(logFilterKey: string): void;
  clearHighlightedBlocks(): void;

  setFlashRanges(ranges: FlashRange[]): void;
  getFlashRanges(): FlashRange[];
  clearFlashRanges(): void;

  // ============================================================================
  // Cursor Position Tracking
  // ============================================================================

  setLastCursorPosition(position: { fileName: string; line: number; column: number; timestamp: number } | null): void;
  getLastCursorPosition(): { fileName: string; line: number; column: number; timestamp: number } | null;

  // ============================================================================
  // Execution Log (for timeline view)
  // ============================================================================

  appendExecutionLog(events: RichExecutionEvent | RichExecutionEvent[]): void;
  clearExecutionLog(): void;
  getExecutionLog(): RichExecutionEvent[];

  // ============================================================================
  // Pending Test Command (for run_test before runtime ready)
  // ============================================================================

  setPendingTestCommand(command: PendingTestCommand | null): void;
  getPendingTestCommand(): PendingTestCommand | null;

  // ============================================================================
  // Pending Function Selection (from URL parameter)
  // ============================================================================

  setPendingFunctionSelection(selection: PendingFunctionSelection | null): void;
  getPendingFunctionSelection(): PendingFunctionSelection | null;
}
