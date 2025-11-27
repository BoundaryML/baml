/**
 * Jotai-based storage implementation
 *
 * Implements SDKStorage interface using Jotai atoms
 */

import type { createStore } from 'jotai';
import type { SDKStorage } from './SDKStorage';
import type {
  ExecutionSnapshot,
  NodeExecutionState,
  NodeExecution,
  CacheEntry,
  BAMLFile,
} from '../types';

// Import atoms directly from core.atoms.ts (no barrel exports)
import {
  runtimeInstanceAtom,
  workflowsAtom,
  activeWorkflowIdAtom,
  workflowExecutionsAtomFamily,
  nodeStateAtomFamily,
  registerNodeAtom,
  cacheAtom,
  getCacheKey,
  versionAtom,
  diagnosticsAtom,
  functionsAtom,
  isRuntimeValid,
  generatedFilesAtom,
  wasmAtom,
  lastValidWasmAtom,
  wasmPanicAtom,
  featureFlagsAtom,
  envVarsAtom,
  bamlFilesAtom,
  bamlFilesTrackedAtom,
  sandboxFilesTrackedAtom,
  vscodeSettingsAtom,
  playgroundPortAtom,
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
  unifiedSelectionStateAtom,
  lastCursorPositionAtom,
  executionLogAtom,
} from '../atoms/core.atoms';

import type { RichExecutionEvent } from '../interface/events';

import type {
  DiagnosticError,
  GeneratedFile,
  WasmPanicState,
  VSCodeSettings,
} from '../atoms/core.atoms';

// Import test execution atoms
import {
  testHistoryAtom,
  selectedHistoryIndexAtom,
  areTestsRunningAtom,
  currentAbortControllerAtom,
  currentWatchNotificationsAtom,
  highlightedBlocksAtom,
  flashRangesAtom,
  pendingTestCommandAtom,
} from '../atoms/test.atoms';

import type {
  TestHistoryRun,
  TestState,
  WatchNotification,
  FlashRange,
  PendingTestCommand,
} from '../atoms/test.atoms';
import type { BamlRuntimeInterface } from '../runtime/BamlRuntimeInterface';
import type { FunctionWithCallGraph } from '../interface';
import { WasmRuntime } from '@gloo-ai/baml-schema-wasm-web';

export class JotaiStorage implements SDKStorage {
  constructor(public store: ReturnType<typeof createStore>) { }

  // ============================================================================
  // Runtime Instance (source of truth for derived state)
  // ============================================================================

  setWasm(wasm: typeof import('@gloo-ai/baml-schema-wasm-web/baml_schema_build') | undefined) {
    this.store.set(wasmAtom, wasm);
  }

  getWasm() {
    return this.store.get(wasmAtom);
  }

  setRuntime(runtime: BamlRuntimeInterface | null) {
    this.store.set(runtimeInstanceAtom, runtime);
  }

  getRuntime() {
    return this.store.get(runtimeInstanceAtom);
  }

  // ============================================================================
  // Workflows
  // ============================================================================

  setWorkflows(workflows: FunctionWithCallGraph[]) {
    this.store.set(workflowsAtom, workflows);
  }

  getWorkflows() {
    return this.store.get(workflowsAtom);
  }

  setActiveWorkflowId(id: string | null) {
    if (id === null) {
      this.store.set(unifiedSelectionStateAtom, { mode: 'empty' });
    } else {
      this.store.set(unifiedSelectionStateAtom, {
        mode: 'workflow',
        workflowId: id,
        selectedNodeId: id,
        functionName: null,
        testName: null,
      });
    }
  }

  getActiveWorkflowId() {
    return this.store.get(activeWorkflowIdAtom);
  }

  // ============================================================================
  // Executions
  // ============================================================================

  addExecution(workflowId: string, execution: ExecutionSnapshot) {
    const executionsAtom = workflowExecutionsAtomFamily(workflowId);
    const executions = this.store.get(executionsAtom);
    this.store.set(executionsAtom, [execution, ...executions]);
  }

  getExecutions(workflowId: string) {
    const executionsAtom = workflowExecutionsAtomFamily(workflowId);
    return this.store.get(executionsAtom);
  }

  updateExecution(executionId: string, updates: Partial<ExecutionSnapshot>) {
    // Find execution across all workflows
    const workflows = this.store.get(workflowsAtom);

    for (const workflow of workflows) {
      const executionsAtom = workflowExecutionsAtomFamily(workflow.id);
      const executions = this.store.get(executionsAtom);
      const index = executions.findIndex((e) => e.id === executionId);

      if (index !== -1) {
        const updated = [...executions];
        updated[index] = { ...updated[index]!, ...updates };
        this.store.set(executionsAtom, updated);
        break;
      }
    }
  }

  // ============================================================================
  // Node States
  // ============================================================================

  setNodeState(nodeId: string, state: NodeExecutionState) {
    // Register node first (ensures atom exists)
    this.store.set(registerNodeAtom, nodeId);
    // Set state
    this.store.set(nodeStateAtomFamily(nodeId), state);
  }

  getNodeState(nodeId: string) {
    return this.store.get(nodeStateAtomFamily(nodeId));
  }

  clearAllNodeStates() {
    // Note: We can't iterate over all atom family instances
    // So we rely on the SDK to track which nodes were used
    // For now, this is a no-op since we can't access all instances
    // The atoms will naturally reset when new execution starts
  }

  // ============================================================================
  // Node Executions (I/O data)
  // ============================================================================

  addNodeExecution(executionId: string, nodeId: string, data: NodeExecution) {
    // Find execution and update its nodeExecutions map
    const workflows = this.store.get(workflowsAtom);

    for (const workflow of workflows) {
      const executionsAtom = workflowExecutionsAtomFamily(workflow.id);
      const executions = this.store.get(executionsAtom);
      const execution = executions.find((e) => e.id === executionId);

      if (execution) {
        execution.nodeExecutions.set(nodeId, data);
        // Trigger reactivity by setting new array
        this.store.set(executionsAtom, [...executions]);
        break;
      }
    }
  }

  getNodeExecution(executionId: string, nodeId: string) {
    const workflows = this.store.get(workflowsAtom);

    for (const workflow of workflows) {
      const executionsAtom = workflowExecutionsAtomFamily(workflow.id);
      const executions = this.store.get(executionsAtom);
      const execution = executions.find((e) => e.id === executionId);

      if (execution) {
        return execution.nodeExecutions.get(nodeId) || null;
      }
    }
    return null;
  }

  // ============================================================================
  // Cache
  // ============================================================================

  setCacheEntry(entry: CacheEntry) {
    const cache = this.store.get(cacheAtom);
    const key = getCacheKey(entry.nodeId, entry.inputsHash);
    cache.set(key, entry);
    this.store.set(cacheAtom, new Map(cache));
  }

  getCacheEntry(nodeId: string, inputsHash: string) {
    const cache = this.store.get(cacheAtom);
    const key = getCacheKey(nodeId, inputsHash);
    return cache.get(key) || null;
  }

  clearCache(scope?: { workflowId?: string; nodeId?: string }) {
    if (!scope) {
      this.store.set(cacheAtom, new Map());
    } else {
      // For now, just clear all
      // TODO: Implement scoped cache clearing
      this.store.set(cacheAtom, new Map());
    }
  }

  // ============================================================================
  // Version
  // ============================================================================

  getVersion() {
    return this.store.get(versionAtom);
  }

  getDiagnostics() {
    return this.store.get(diagnosticsAtom);
  }



  getFunctions() {
    return this.store.get(functionsAtom);
  }

  // ============================================================================
  // Generated Files
  // ============================================================================

  getGeneratedFiles() {
    return this.store.get(generatedFilesAtom);
  }

  // ============================================================================
  // WASM Instance
  // ============================================================================

  setWasmRuntime(wasm: WasmRuntime | undefined) {
    this.store.set(lastValidWasmAtom, wasm);
  }

  getWasmRuntime() {
    return this.store.get(lastValidWasmAtom);
  }

  // ============================================================================
  // WASM Panic
  // ============================================================================

  setWasmPanic(panic: WasmPanicState | null) {
    this.store.set(wasmPanicAtom, panic);
  }

  getWasmPanic() {
    return this.store.get(wasmPanicAtom);
  }

  // ============================================================================
  // Feature Flags
  // ============================================================================

  setFeatureFlags(flags: string[]) {
    this.store.set(featureFlagsAtom, flags);
  }

  getFeatureFlags() {
    return this.store.get(featureFlagsAtom);
  }

  // ============================================================================
  // Environment Variables
  // ============================================================================

  setEnvVars(envVars: Record<string, string>) {
    this.store.set(envVarsAtom, envVars);
  }

  getEnvVars() {
    return this.store.get(envVarsAtom);
  }

  // ============================================================================
  // Files Tracking
  // ============================================================================

  setBAMLFiles(files: Record<string, string>) {
    this.store.set(bamlFilesTrackedAtom, files);
  }

  getBAMLFiles() {
    return this.store.get(bamlFilesTrackedAtom);
  }

  setParsedBAMLFiles(files: BAMLFile[]) {
    this.store.set(bamlFilesAtom, files);
  }

  setSandboxFiles(files: Record<string, string>) {
    this.store.set(sandboxFilesTrackedAtom, files);
  }

  getSandboxFiles() {
    return this.store.get(sandboxFilesTrackedAtom);
  }

  // ============================================================================
  // VSCode Integration
  // ============================================================================

  setVSCodeSettings(settings: VSCodeSettings | null) {
    this.store.set(vscodeSettingsAtom, settings);
  }

  getVSCodeSettings() {
    return this.store.get(vscodeSettingsAtom);
  }

  setPlaygroundPort(port: number) {
    this.store.set(playgroundPortAtom, port);
  }

  getPlaygroundPort() {
    return this.store.get(playgroundPortAtom);
  }

  // ============================================================================
  // Selection State (Function & Test Case)
  // ============================================================================

  getSelectedFunctionName() {
    return this.store.get(selectedFunctionNameAtom);
  }

  getSelectedTestCaseName() {
    return this.store.get(selectedTestCaseNameAtom);
  }

  getUnifiedSelectionState() {
    return this.store.get(unifiedSelectionStateAtom);
  }

  // ============================================================================
  // Test Execution State
  // ============================================================================

  addTestHistoryRun(run: TestHistoryRun) {
    const current = this.store.get(testHistoryAtom);
    this.store.set(testHistoryAtom, [run, ...current]);
  }

  getTestHistory() {
    return this.store.get(testHistoryAtom);
  }

  updateTestInHistory(runIndex: number, testIndex: number, update: TestState) {
    console.log('[JotaiStorage] updateTestInHistory called:', { runIndex, testIndex, update });
    this.store.set(testHistoryAtom, (prev) => {
      console.log('[JotaiStorage] inside functional updater, prev length:', prev.length);
      const newHistory = [...prev];
      const run = newHistory[runIndex];
      if (!run) {
        console.warn('[JotaiStorage] run not found at index:', runIndex);
        return prev;
      }

      const test = run.tests[testIndex];
      if (!test) {
        console.warn('[JotaiStorage] test not found at index:', testIndex);
        return prev;
      }

      run.tests[testIndex] = {
        ...test,
        response: update,
        timestamp: Date.now(),
      };

      console.log('[JotaiStorage] updated test:', run.tests[testIndex]);
      return newHistory;
    });
  }

  setSelectedHistoryIndex(index: number) {
    this.store.set(selectedHistoryIndexAtom, index);
  }

  getSelectedHistoryIndex() {
    return this.store.get(selectedHistoryIndexAtom);
  }

  setAreTestsRunning(running: boolean) {
    this.store.set(areTestsRunningAtom, running);
  }

  getAreTestsRunning() {
    return this.store.get(areTestsRunningAtom);
  }

  setCurrentAbortController(controller: AbortController | null) {
    this.store.set(currentAbortControllerAtom, controller);
  }

  getCurrentAbortController() {
    return this.store.get(currentAbortControllerAtom);
  }

  setCurrentWatchNotifications(notifications: WatchNotification[]) {
    this.store.set(currentWatchNotificationsAtom, notifications);
  }

  getCurrentWatchNotifications() {
    return this.store.get(currentWatchNotificationsAtom);
  }

  addWatchNotification(notification: WatchNotification) {
    const current = this.store.get(currentWatchNotificationsAtom);
    this.store.set(currentWatchNotificationsAtom, [...current, notification]);
  }

  clearWatchNotifications() {
    this.store.set(currentWatchNotificationsAtom, []);
  }

  setHighlightedBlocks(blocks: Set<string>) {
    this.store.set(highlightedBlocksAtom, blocks);
  }

  getHighlightedBlocks() {
    return this.store.get(highlightedBlocksAtom);
  }

  addHighlightedBlock(lexicalNodeId: string) {
    const current = this.store.get(highlightedBlocksAtom);
    const newSet = new Set(current);
    newSet.add(lexicalNodeId);
    this.store.set(highlightedBlocksAtom, newSet);
  }

  clearHighlightedBlocks() {
    this.store.set(highlightedBlocksAtom, new Set());
  }

  setFlashRanges(ranges: FlashRange[]) {
    this.store.set(flashRangesAtom, ranges);
  }

  getFlashRanges() {
    return this.store.get(flashRangesAtom);
  }

  clearFlashRanges() {
    this.store.set(flashRangesAtom, []);
  }

  // ============================================================================
  // Cursor Position Tracking
  // ============================================================================

  setLastCursorPosition(position: { fileName: string; line: number; column: number; timestamp: number } | null) {
    this.store.set(lastCursorPositionAtom, position);
  }

  getLastCursorPosition() {
    return this.store.get(lastCursorPositionAtom);
  }

  // ============================================================================
  // Execution Log (for timeline view)
  // ============================================================================

  appendExecutionLog(events: RichExecutionEvent | RichExecutionEvent[]) {
    const currentLog = this.store.get(executionLogAtom);
    const newEvents = Array.isArray(events) ? events : [events];
    this.store.set(executionLogAtom, [...currentLog, ...newEvents]);
  }

  clearExecutionLog() {
    this.store.set(executionLogAtom, []);
  }

  getExecutionLog() {
    return this.store.get(executionLogAtom);
  }

  // ============================================================================
  // Pending Test Command
  // ============================================================================

  setPendingTestCommand(command: PendingTestCommand | null) {
    this.store.set(pendingTestCommandAtom, command);
  }

  getPendingTestCommand() {
    return this.store.get(pendingTestCommandAtom);
  }
}
