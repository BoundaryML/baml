import { atom } from 'jotai';
import type { Getter, Setter } from 'jotai';
import type { CodeClickEvent, NavigationIntent } from '../types';
import { determineNavigationAction, type NavigationContext, isWorkflowWithStructure } from '../navigationHeuristic';
import {
  unifiedSelectionStateAtom,
  detailPanelAtom,
  selectedInputSourceAtom,
  functionsAtom,
  bamlFilesAtom,
  allFunctionsMapAtom,
  workflowsAtom,
  type SelectionState,
  type WorkflowSelection,
  type FunctionSelection,
} from '../atoms/core.atoms';
import { activeTabAtom } from '../../shared/baml-project-panel/playground-panel/unified-atoms';
import type { FunctionWithCallGraph } from '../interface';
import { flowStore } from '../../states/reactflow';
import { panToNodeIfNeeded } from '../../utils/cameraPan';

const pendingTimeouts = new Set<ReturnType<typeof setTimeout>>();

function clearPendingTimeouts() {
  pendingTimeouts.forEach(clearTimeout);
  pendingTimeouts.clear();
}

function schedule(fn: () => void, delay: number) {
  const id = setTimeout(() => {
    pendingTimeouts.delete(id);
    fn();
  }, delay);
  pendingTimeouts.add(id);
}

function buildNavigationState(get: Getter): NavigationContext {
  const selection = get(unifiedSelectionStateAtom);
  return {
    activeWorkflowId: selection.mode === 'workflow' ? selection.workflowId : null,
    // Use workflowsAtom to get only multi-node workflows (excludes standalone functions)
    // The navigation heuristic searches through these to find which workflow contains a function
    workflows: get(workflowsAtom),
    bamlFiles: get(bamlFilesAtom) ?? [],
  };
}

function openDetailPanel(set: Setter) {
  set(detailPanelAtom, (prev: any) => ({ ...prev, isOpen: true }));
}

function closeDetailPanel(set: Setter) {
  set(detailPanelAtom, (prev: any) => ({ ...prev, isOpen: false }));
}

function setSelection(
  set: Setter,
  state: SelectionState,
) {
  set(unifiedSelectionStateAtom, state);
}

function resolveNodeId(workflow: FunctionWithCallGraph, targetId: string): string {
  const directMatch = workflow.nodes?.find((node) => node.id === targetId);
  if (directMatch) return directMatch.id;

  const labelMatch = workflow.nodes?.find((node) => node.label === targetId);
  if (labelMatch) return labelMatch.id;

  const rootMatch = workflow.nodes?.find((node) => node.id?.startsWith(`${targetId}|`));
  if (rootMatch) return rootMatch.id;

  return targetId;
}

function selectTestInput(
  set: Setter,
  nodeId: string,
  testId: string | undefined,
) {
  if (!testId) {
    set(selectedInputSourceAtom, null);
    return;
  }

  set(selectedInputSourceAtom, {
    nodeId,
    sourceType: 'test',
    sourceId: testId,
  });
}

function panToNode(nodeId: string) {
  if (!flowStore.value.initialized) {
    return false;
  }
  const nodes = flowStore.value.getNodes?.() ?? [];
  const node = nodes.find((n) => n.id === nodeId);
  if (node) {
    panToNodeIfNeeded(node, flowStore.value);
    return true;
  }
  return false;
}

function waitForNode(
  nodeId: string,
  attemptsLeft: number,
  onSuccess: () => void,
) {
  if (panToNode(nodeId)) {
    onSuccess();
    return;
  }

  if (attemptsLeft <= 0) {
    console.warn('⚠️ Node not found in ReactFlow after switching:', nodeId);
    return;
  }

  schedule(() => waitForNode(nodeId, attemptsLeft - 1, onSuccess), 100);
}

function applyNavigationAction(
  action: SelectionState,
  intent: NavigationIntent,
  get: Getter,
  set: Setter,
) {
  switch (action.mode) {
    case 'workflow': {
      console.log('🔄 Workflow mode:', action.workflowId, '→', action.selectedNodeId);

      // Auto-select first test if not specified
      let testName = action.testName;
      if (!testName) {
        const allFunctions: Map<string, FunctionWithCallGraph> = get(allFunctionsMapAtom);
        const func = allFunctions.get(action.selectedNodeId);
        if (func && func.testCases && func.testCases.length > 0) {
          testName = func.testCases[0]!.name;
          console.log('  → Auto-selecting first test:', testName);
        }
      }

      const finalState: WorkflowSelection = {
        mode: 'workflow',
        workflowId: action.workflowId,
        selectedNodeId: action.selectedNodeId,
        testName,
      };

      setSelection(set, finalState);
      set(activeTabAtom, 'graph');
      selectTestInput(set, action.selectedNodeId, testName ?? undefined);
      openDetailPanel(set);

      // Pan to node after a short delay
      schedule(() => panToNode(action.selectedNodeId), 100);
      break;
    }

    case 'function': {
      console.log('📝 Function mode:', action.functionName);

      // Auto-select first test if not specified
      let testName = action.testName;
      if (!testName) {
        const allFunctions: Map<string, FunctionWithCallGraph> = get(allFunctionsMapAtom);
        const func = allFunctions.get(action.functionName);
        if (func && func.testCases && func.testCases.length > 0) {
          testName = func.testCases[0]!.name;
          console.log('  → Auto-selecting first test:', testName);
        }
      }

      const finalState: FunctionSelection = {
        mode: 'function',
        functionName: action.functionName,
        testName,
      };

      setSelection(set, finalState);
      set(activeTabAtom, 'preview');
      selectTestInput(set, action.functionName, testName ?? undefined);
      openDetailPanel(set);
      break;
    }

    case 'empty': {
      console.log('📭 Empty state');
      setSelection(set, { mode: 'empty' });
      set(activeTabAtom, 'preview');
      selectTestInput(set, '', undefined);
      closeDetailPanel(set);
      break;
    }
  }
}

export const navigationIntentAtom = atom<NavigationIntent | null>(null);

function toCodeClickEvent(intent: NavigationIntent): CodeClickEvent {
  const filePath = intent.filePath ?? 'unknown';
  if (intent.type === 'function') {
    return {
      type: 'function',
      functionName: intent.functionName,
      functionType: intent.functionType,
      filePath,
    };
  }

  return {
    type: 'test',
    functionName: intent.functionName,
    testName: intent.testName,
    nodeType: intent.nodeType,
    filePath,
  };
}

export const navigationDispatcherAtom = atom(
  null,
  (get, set, intent: NavigationIntent) => {
    clearPendingTimeouts();
    set(navigationIntentAtom, intent);

    const navState = buildNavigationState(get);
    const action = determineNavigationAction(toCodeClickEvent(intent), navState);
    applyNavigationAction(action, intent, get, set);
  }
);
