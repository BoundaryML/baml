/**
 * React Hooks for BAML SDK
 *
 * Convenient hooks for React components to access SDK state and functionality
 */

import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { useCallback, useMemo, useContext } from 'react';
import { BAMLSDKContext } from './context';
import type { BAMLSDK } from './sdk';

// Re-export provider component for convenience
export { BAMLSDKProvider } from './provider';

/**
 * Hook to access the SDK instance
 */
export function useBAMLSDK(): BAMLSDK {
  const sdk = useContext(BAMLSDKContext);
  if (!sdk) {
    throw new Error('useBAMLSDK must be used within BAMLSDKProvider');
  }
  return sdk;
}


// Import atoms directly from core.atoms.ts (no barrel exports)
import {
  workflowsAtom,
  activeWorkflowIdAtom,
  activeWorkflowAtom,
  activeWorkflowExecutionsAtom,
  selectedExecutionIdAtom,
  selectedExecutionAtom,
  latestExecutionAtom,
  currentGraphAtom,
  nodeExecutionsAtom,
  nodeStateAtomFamily,
  allNodeStatesAtom,
  allNodeIterationsAtom,
  nodeIterationAtomFamily,
  viewModeAtom,
  selectedNodeIdAtom,
  detailPanelAtom,
  layoutDirectionAtom,
  eventStreamAtom,
  selectedInputSourceAtom,
  activeNodeInputsAtom,
  inputsDirtyAtom,
  allFunctionsMapAtom,
  unifiedSelectionStateAtom,
  // New atoms from migration
  diagnosticsAtom,
  numErrorsAtom,
  isRuntimeValid,
  generatedFilesAtom,
  generatedFilesByLangAtomFamily,
  wasmPanicAtom,
  featureFlagsAtom,
  betaFeatureEnabledAtom,
  envVarsAtom,
  bamlFilesTrackedAtom,
  sandboxFilesTrackedAtom,
  vscodeSettingsAtom,
  playgroundPortAtom,
  proxyUrlAtom,
} from './atoms/core.atoms';

import type { BAMLEvent, InputSource, NodeType } from './types';

// ============================================================================
// Workflow Hooks
// ============================================================================

/**
 * Get all available workflows
 */
export function useWorkflows() {
  return useAtomValue(workflowsAtom);
}

/**
 * Get and set the active workflow
 */
export function useActiveWorkflow() {
  const activeWorkflowId = useAtomValue(activeWorkflowIdAtom);
  const activeWorkflow = useAtomValue(activeWorkflowAtom);
  const workflows = useAtomValue(workflowsAtom);
  const [, setUnifiedSelection] = useAtom(unifiedSelectionStateAtom);

  const setActive = useCallback(
    (workflowId: string | null) => {
      if (workflowId === null) {
        setUnifiedSelection((prev) => ({ ...prev, activeWorkflowId: null }));
        return;
      }

      const workflow = workflows.find((w) => w.id === workflowId);
      if (workflow) {
        setUnifiedSelection((prev) => ({ ...prev, activeWorkflowId: workflowId }));
      } else {
        console.warn(`⚠️ Cannot set active workflow: "${workflowId}" not found`);
      }
    },
    [workflows, setUnifiedSelection]
  );

  return {
    activeWorkflow,
    activeWorkflowId,
    setActiveWorkflow: setActive,
  };
}

// ============================================================================
// Execution Hooks
// ============================================================================

/**
 * Get executions for the active workflow
 */
export function useActiveWorkflowExecutions() {
  return useAtomValue(activeWorkflowExecutionsAtom);
}

/**
 * Get the selected execution (for viewing snapshots)
 */
export function useSelectedExecution() {
  const [selectedExecutionId, setSelectedExecutionId] = useAtom(
    selectedExecutionIdAtom
  );
  const selectedExecution = useAtomValue(selectedExecutionAtom);

  return {
    selectedExecution,
    selectedExecutionId,
    setSelectedExecutionId,
  };
}

/**
 * Get the latest execution for the active workflow
 */
export function useLatestExecution() {
  return useAtomValue(latestExecutionAtom);
}

// ============================================================================
// Node Hooks
// ============================================================================

/**
 * Get node execution data
 */
export function useNodeExecutions() {
  return useAtomValue(nodeExecutionsAtom);
}

/**
 * Get execution data for a specific node
 */
export function useNodeExecution(nodeId: string) {
  const nodeExecutions = useNodeExecutions();
  return nodeExecutions.get(nodeId) ?? null;
}

/**
 * Get state for a specific node using atomFamily
 *
 * This provides granular subscriptions - components only re-render
 * when their specific node's state changes.
 */
export function useNodeState(nodeId: string) {
  return useAtomValue(nodeStateAtomFamily(nodeId));
}

/**
 * Set state for a specific node
 */
export function useSetNodeState(nodeId: string) {
  return useSetAtom(nodeStateAtomFamily(nodeId));
}

/**
 * Get all node states as a Map
 *
 * Use this only when you need to iterate over all nodes.
 * For single-node subscriptions, use useNodeState() instead for better performance.
 */
export function useAllNodeStates() {
  return useAtomValue(allNodeStatesAtom);
}

/**
 * Get iteration count for a specific node
 * Used to track how many times a node has run in a loop
 */
export function useNodeIteration(nodeId: string) {
  return useAtomValue(nodeIterationAtomFamily(nodeId));
}

/**
 * Get all node iterations as a Map
 * Used for displaying iteration badges on nodes
 */
export function useAllNodeIterations() {
  return useAtomValue(allNodeIterationsAtom);
}

// ============================================================================
// UI State Hooks
// ============================================================================

/**
 * Get and set view mode (editor vs execution snapshot)
 */
export function useViewMode() {
  return useAtom(viewModeAtom);
}


/**
 * Get and set detail panel state
 */
export function useDetailPanel() {
  const panel = useAtomValue(detailPanelAtom);
  const setPanel = useSetAtom(detailPanelAtom);

  const open = useCallback(() => {
    setPanel((prev) => ({ ...prev, isOpen: true }));
  }, [setPanel]);

  const close = useCallback(() => {
    setPanel((prev) => ({ ...prev, isOpen: false }));
  }, [setPanel]);

  const setPosition = useCallback(
    (position: 'bottom' | 'right' | 'floating') => {
      setPanel((prev) => ({ ...prev, position }));
    },
    [setPanel]
  );

  const setActiveTab = useCallback(
    (tab: 'io' | 'logs' | 'history') => {
      setPanel((prev) => ({ ...prev, activeTab: tab }));
    },
    [setPanel]
  );

  return {
    ...panel,
    open,
    close,
    setPosition,
    setActiveTab,
  };
}

/**
 * Get and set layout direction
 */
export function useLayoutDirection() {
  return useAtom(layoutDirectionAtom);
}

// ============================================================================
// Event Hooks
// ============================================================================

/**
 * Get recent events
 */
export function useEvents(limit = 10) {
  const events = useAtomValue(eventStreamAtom);
  return events.slice(-limit);
}

/**
 * Subscribe to specific event types
 */
export function useEventSubscription(
  eventType: BAMLEvent['type'],
  callback: (event: BAMLEvent) => void
) {
  const events = useAtomValue(eventStreamAtom);

  // Only call callback for new events of the specified type
  useMemo(() => {
    const latestEvent = events[events.length - 1];
    if (latestEvent && latestEvent.type === eventType) {
      callback(latestEvent);
    }
  }, [events, eventType, callback]);
}

// ============================================================================
// Combined/Derived Hooks
// ============================================================================

/**
 * Get the current graph to display (editor or snapshot)
 * Now reads from currentGraphAtom for better reactivity
 */
export function useCurrentGraph() {
  return useAtomValue(currentGraphAtom);
}

/**
 * Get the active node (selected node with its execution data)
 */
export function useActiveNode() {
  const selectedNodeId = useAtomValue(selectedNodeIdAtom);
  const nodeExecutions = useNodeExecutions();
  const currentGraph = useCurrentGraph();
  const allFunctions = useAtomValue(allFunctionsMapAtom);

  // IMPORTANT: Call all hooks before any conditional returns (Rules of Hooks)
  // Use empty string as fallback to maintain hook call order even when no node is selected
  const state = useNodeState(selectedNodeId ?? '');

  if (!selectedNodeId) return null;

  // First try to find node in current graph (for workflow nodes)
  let node = currentGraph.nodes.find((n) => n.id === selectedNodeId);



  // If not found in graph, check if it's a standalone function
  if (!node) {
    const func = allFunctions.get(selectedNodeId);
    if (func) {
      // Create a synthetic node for standalone function
      // Map function type to NodeType (workflows become 'function' for synthetic nodes)
      const nodeType: NodeType =
        func.type === 'llm_function' ? 'llm_function' :
          'function'; // default for 'function' and 'workflow' types

      node = {
        id: func.name,
        label: func.name,
        type: nodeType,
        functionName: func.name,
        codeHash: '', // Empty for synthetic nodes
        lastModified: Date.now(),
      };
    }
  }

  if (!node) return null;

  const execution = nodeExecutions.get(selectedNodeId);

  return {
    node,
    execution,
    state,
  };
}

// ============================================================================
// Input Library Hooks (Phase 1: Previous Executions)
// ============================================================================

/**
 * Get available input sources for a node
 * Includes both previous executions and test cases
 */
export function useNodeInputSources(nodeId: string): InputSource[] {
  const workflowExecutions = useAtomValue(activeWorkflowExecutionsAtom);

  return useMemo(() => {
    if (!workflowExecutions.length) return [];
    const inputSources: InputSource[] = [];

    // Build input sources from executions where this node ran
    workflowExecutions.forEach((execution, index) => {
      const nodeExec = execution.nodeExecutions.get(nodeId);
      if (nodeExec && nodeExec.inputs) {
        inputSources.push({
          id: execution.id,
          name: `Execution #${workflowExecutions.length - index}`,
          source: 'execution',
          nodeId,
          executionId: execution.id,
          timestamp: execution.timestamp,
          inputs: nodeExec.inputs,
          outputs: nodeExec.outputs,
          status: nodeExec.state === 'success' ? 'success' : nodeExec.state === 'error' ? 'error' : 'running',
        });
      }
    });

    return inputSources;
  }, [workflowExecutions, nodeId]);
}

/**
 * Get and set the selected input source
 */
export function useSelectedInputSource() {
  const [selectedSource, setSelectedSource] = useAtom(selectedInputSourceAtom);

  const selectSource = useCallback(
    (nodeId: string, sourceType: 'execution' | 'test' | 'manual', sourceId: string) => {
      setSelectedSource({ nodeId, sourceType, sourceId });
    },
    [setSelectedSource]
  );

  const clearSource = useCallback(() => {
    setSelectedSource(null);
  }, [setSelectedSource]);

  return {
    selectedSource,
    selectSource,
    clearSource,
  };
}

/**
 * Get active inputs for a node
 * Returns inputs from selected source or latest execution
 */
export function useNodeActiveInputs(nodeId: string): Record<string, any> {
  const selectedSource = useAtomValue(selectedInputSourceAtom);
  const inputSources = useNodeInputSources(nodeId);
  const latestExecution = useLatestExecution();

  return useMemo(() => {
    // If a source is selected for this node, use it
    if (selectedSource && selectedSource.nodeId === nodeId) {
      const source = inputSources.find((s) => s.id === selectedSource.sourceId);
      if (source) {
        return source.inputs;
      }
    }

    // Otherwise, fall back to latest execution
    if (latestExecution) {
      const nodeExec = latestExecution.nodeExecutions.get(nodeId);
      if (nodeExec?.inputs) {
        return nodeExec.inputs;
      }
    }

    return {};
  }, [selectedSource, inputSources, latestExecution, nodeId]);
}

/**
 * Get/set active node inputs (editable)
 */
export function useActiveNodeInputs() {
  const [inputs, setInputs] = useAtom(activeNodeInputsAtom);
  const [isDirty, setIsDirty] = useAtom(inputsDirtyAtom);

  const updateInputs = useCallback(
    (newInputs: Record<string, any>) => {
      setInputs(newInputs);
      setIsDirty(true);
    },
    [setInputs, setIsDirty]
  );

  const resetInputs = useCallback(() => {
    setInputs({});
    setIsDirty(false);
  }, [setInputs, setIsDirty]);

  return {
    inputs,
    updateInputs,
    resetInputs,
    isDirty,
  };
}

// ============================================================================
// Derived State Hooks (Function Lookup & LLM Mode)
// Per design document: compute derived state in hooks with useMemo instead of atoms
// ============================================================================

/**
 * Get all functions as a Map for O(1) lookup
 * Uses the cached atom for performance
 */
export function useAllFunctions() {
  return useAtomValue(allFunctionsMapAtom);
}

/**
 * Get functions grouped by type
 * Computed on-demand with useMemo instead of a cached atom
 */
export function useFunctionsByType() {
  const allFunctions = useAtomValue(allFunctionsMapAtom);

  return useMemo(() => {
    const byType: Record<string, any[]> = {
      workflow: [],
      llm_function: [],
      function: [],
    };

    for (const func of allFunctions.values()) {
      if (!byType[func.type]) {
        byType[func.type] = [];
      }
      byType[func.type]?.push(func);
    }

    return byType;
  }, [allFunctions]);
}

/**
 * Get standalone functions (not in any workflow)
 * Computed on-demand with useMemo instead of a cached atom
 */
export function useStandaloneFunctions() {
  const allFunctions = useAtomValue(allFunctionsMapAtom);
  const workflows = useAtomValue(workflowsAtom);

  return useMemo(() => {
    // Build set of workflow function IDs
    const workflowFunctionIds = new Set<string>();
    for (const workflow of workflows) {
      for (const node of workflow.nodes) {
        workflowFunctionIds.add(node.id);
      }
    }

    // Filter to standalone functions
    const standalone = new Map<string, any>();
    for (const [name, func] of allFunctions) {
      if (!workflowFunctionIds.has(name)) {
        standalone.set(name, func);
      }
    }

    return standalone;
  }, [allFunctions, workflows]);
}

/**
 * Get the currently selected function details
 * Computed on-demand with useMemo instead of a cached atom
 */
export function useSelectedFunction() {
  const selectedNodeId = useAtomValue(selectedNodeIdAtom);
  const allFunctions = useAtomValue(allFunctionsMapAtom);

  return useMemo(() => {
    if (!selectedNodeId) return null;
    return allFunctions.get(selectedNodeId) ?? null;
  }, [selectedNodeId, allFunctions]);
}

/**
 * Check if we're in LLM-only mode
 * Computed on-demand with useMemo instead of a cached atom
 *
 * True when:
 * 1. Selected node is an LLM function
 * 2. NOT part of any workflow
 */
export function useLLMOnlyMode() {
  const selectedNodeId = useAtomValue(selectedNodeIdAtom);
  const activeWorkflow = useAtomValue(activeWorkflowAtom);
  const allFunctions = useAtomValue(allFunctionsMapAtom);
  const workflows = useAtomValue(workflowsAtom);

  return useMemo(() => {
    if (!selectedNodeId) return false;

    // Check if it's an LLM function
    let isLLMFunction = false;

    // Option 1: Check in current graph (if function is part of active workflow)
    if (activeWorkflow) {
      const node = activeWorkflow.nodes.find((n) => n.id === selectedNodeId);
      if (node?.type === 'llm_function') {
        isLLMFunction = true;
      }
    }

    // Option 2: Check in all functions (standalone functions)
    if (!isLLMFunction) {
      const selectedFunction = allFunctions.get(selectedNodeId);
      if (selectedFunction?.type === 'llm_function') {
        isLLMFunction = true;
      }
    }

    // If not an LLM function, definitely not LLM-only mode
    if (!isLLMFunction) return false;

    // Check if part of any workflow
    for (const workflow of workflows) {
      for (const node of workflow.nodes) {
        if (node.id === selectedNodeId) {
          return false; // Found in a workflow, not standalone
        }
      }
    }

    return true; // LLM function not in any workflow
  }, [selectedNodeId, activeWorkflow, allFunctions, workflows]);
}

// ============================================================================
// Diagnostics Hooks
// ============================================================================

/**
 * Get compilation diagnostics (errors and warnings)
 */
export function useDiagnostics() {
  return useAtomValue(diagnosticsAtom);
}

/**
 * Get error and warning counts
 */
export function useErrorCounts() {
  return useAtomValue(numErrorsAtom);
}

/**
 * Check if runtime is valid (no compilation errors)
 */
export function useIsRuntimeValid() {
  return useAtomValue(isRuntimeValid);
}

// ============================================================================
// Generated Files Hooks
// ============================================================================

/**
 * Get all generated files
 */
export function useGeneratedFiles() {
  return useAtomValue(generatedFilesAtom);
}

/**
 * Get generated files filtered by language
 */
export function useGeneratedFilesByLanguage(lang: string) {
  return useAtomValue(generatedFilesByLangAtomFamily(lang));
}

// ============================================================================
// WASM Panic Hooks
// ============================================================================

/**
 * Get WASM panic state
 */
export function useWasmPanic() {
  return useAtomValue(wasmPanicAtom);
}

/**
 * Clear WASM panic
 */
export function useClearWasmPanic() {
  const setPanic = useSetAtom(wasmPanicAtom);
  return useCallback(() => setPanic(null), [setPanic]);
}

// ============================================================================
// Feature Flags Hooks
// ============================================================================

/**
 * Get feature flags
 */
export function useFeatureFlags() {
  return useAtomValue(featureFlagsAtom);
}

/**
 * Check if beta features are enabled
 */
export function useBetaFeatureEnabled() {
  return useAtomValue(betaFeatureEnabledAtom);
}

// ============================================================================
// Environment Variables Hooks
// ============================================================================

/**
 * Get environment variables
 */
export function useEnvVars() {
  return useAtomValue(envVarsAtom);
}

// ============================================================================
// Files Tracking Hooks
// ============================================================================

/**
 * Get tracked BAML files
 */
export function useBAMLFiles() {
  return useAtomValue(bamlFilesTrackedAtom);
}

/**
 * Get sandbox files
 */
export function useSandboxFiles() {
  return useAtomValue(sandboxFilesTrackedAtom);
}

// ============================================================================
// VSCode Integration Hooks
// ============================================================================

/**
 * Get VSCode settings
 */
export function useVSCodeSettings() {
  return useAtomValue(vscodeSettingsAtom);
}

/**
 * Get playground port
 */
export function usePlaygroundPort() {
  return useAtomValue(playgroundPortAtom);
}

/**
 * Get proxy URL configuration
 */
export function useProxyUrl() {
  return useAtomValue(proxyUrlAtom);
}

// ============================================================================
// Navigation Hooks
// ============================================================================

import type { NavigationInput } from './navigation';

/**
 * Hook to dispatch navigation events
 *
 * Usage:
 * ```tsx
 * const navigate = useNavigation();
 *
 * navigate({
 *   kind: 'function',
 *   functionName: 'MyFunction',
 *   source: 'debug-panel',
 *   timestamp: Date.now(),
 * });
 * ```
 */
export function useNavigation() {
  const sdk = useBAMLSDK();
  return (input: NavigationInput) => sdk.navigate(input);
}
