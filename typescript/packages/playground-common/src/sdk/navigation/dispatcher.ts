import { atom } from 'jotai';
import type { Getter, Setter } from 'jotai';
import type { CodeClickEvent, NavigationIntent } from '../types';
import { determineNavigationAction, type NavigationAction, type NavigationState } from '../navigationHeuristic';
import {
  unifiedSelectionStateAtom,
  detailPanelAtom,
  selectedInputSourceAtom,
  workflowsAtom,
  bamlFilesAtom,
  allFunctionsMapAtom,
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

function buildNavigationState(get: Getter): NavigationState {
  const selection = get(unifiedSelectionStateAtom);
  return {
    activeWorkflowId: selection.activeWorkflowId,
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
  updater: (prev: any) => any,
) {
  set(unifiedSelectionStateAtom, updater);
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
  action: NavigationAction,
  intent: NavigationIntent,
  get: Getter,
  set: Setter,
) {
  switch (action.type) {
    case 'switch-workflow': {
      console.log('🔄 Switching to workflow:', action.workflowId);

      // Auto-select first test if available
      let testName = null;
      const allFunctions: Map<string, FunctionWithCallGraph> = get(allFunctionsMapAtom);
      const func = allFunctions.get(action.workflowId);
      if (func && func.testCases && func.testCases.length > 0) {
        testName = func.testCases[0]!.name;
        console.log('  → Auto-selecting first test:', testName);
      }

      setSelection(set, (prev: any) => ({
        ...prev,
        functionName: action.workflowId,
        testName,
        activeWorkflowId: action.workflowId,
        selectedNodeId: action.workflowId,
      }));
      set(activeTabAtom, 'graph');
      selectTestInput(set, action.workflowId, testName ?? undefined);
      openDetailPanel(set);
      break;
    }

    case 'select-node': {
      console.log('🎯 Selecting node in current workflow:', action.nodeId);

      // Auto-select first test if no test specified
      let testName = action.testId ?? null;
      if (!testName) {
        const allFunctions: Map<string, FunctionWithCallGraph> = get(allFunctionsMapAtom);
        const func = allFunctions.get(action.nodeId);
        if (func && func.testCases.length > 0) {
          testName = func.testCases[0]!.name;
          console.log('  → Auto-selecting first test:', testName);
        }
      }

      setSelection(set, (prev: any) => ({
        ...prev,
        functionName: action.nodeId,
        testName,
        activeWorkflowId: action.workflowId,
        selectedNodeId: action.nodeId,
      }));
      set(activeTabAtom, 'graph');
      openDetailPanel(set);
      selectTestInput(set, action.nodeId, testName ?? undefined);
      schedule(() => panToNode(action.nodeId), 100);
      break;
    }

    case 'switch-and-select': {
      console.log('🔄 Switching to workflow and selecting node:', action.workflowId, '→', action.nodeId);
      const workflows = get(workflowsAtom) as FunctionWithCallGraph[];
      const workflow = workflows.find((wf) => wf.id === action.workflowId);
      if (!workflow) {
        console.error(`❌ Cannot switch to workflow: "${action.workflowId}" not found`);
        setSelection(set, () => ({
          functionName: null,
          testName: null,
          activeWorkflowId: null,
          selectedNodeId: null,
        }));
        selectTestInput(set, '', undefined);
        closeDetailPanel(set);
        return;
      }

      const targetNodeId = resolveNodeId(workflow, action.nodeId);

      // Auto-select first test if no test specified
      let testName = action.testId ?? null;
      if (!testName) {
        const allFunctions: Map<string, FunctionWithCallGraph> = get(allFunctionsMapAtom);
        const func = allFunctions.get(action.nodeId);
        if (func && func.testCases.length > 0) {
          testName = func.testCases[0]!.name;
          console.log('  → Auto-selecting first test:', testName);
        }
      }

      setSelection(set, (prev: any) => ({
        ...prev,
        functionName: action.nodeId,
        testName,
        activeWorkflowId: action.workflowId,
        selectedNodeId: action.nodeId,
      }));
      set(activeTabAtom, 'graph');

      const finalizeSelection = () => {
        setSelection(set, (prev: any) => ({
          ...prev,
          selectedNodeId: targetNodeId,
        }));
        openDetailPanel(set);
        selectTestInput(set, targetNodeId, testName ?? undefined);
      };

      schedule(() => {
        if (panToNode(targetNodeId)) {
          finalizeSelection();
        } else {
          waitForNode(targetNodeId, 20, finalizeSelection);
        }
      }, 150);
      break;
    }

    case 'show-function-tests': {
      console.log('📝 Showing function with tests:', action.functionName);
      setSelection(set, (prev: any) => ({
        ...prev,
        functionName: action.functionName,
        testName: action.tests[0] ?? null,
        activeWorkflowId: null,
        selectedNodeId: action.functionName,
      }));
      set(activeTabAtom, 'preview');
      selectTestInput(set, '', undefined);
      openDetailPanel(set);
      break;
    }

    case 'empty-state': {
      console.log('📭 Empty state:', action.reason, intent.functionName);
      setSelection(set, (prev: any) => ({
        ...prev,
        functionName: intent.functionName ?? null,
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      }));
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
