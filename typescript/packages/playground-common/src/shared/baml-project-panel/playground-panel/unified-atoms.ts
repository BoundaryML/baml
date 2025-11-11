/**
 * Unified State Atoms
 *
 * This file contains the unified state atoms that merge the state management
 * between the original PromptPreview app and the WorkflowApp.
 */

import { atom } from 'jotai';
import { selectionAtom as originalSelectionAtom } from './atoms';
import {
  unifiedSelectionStateAtom,
  type UnifiedSelectionState,
  activeWorkflowAtom,
} from '../../../sdk/atoms/core.atoms';

// ============================================================================
// CORE UNIFIED STATE
// ============================================================================

/**
 * Unified selection state - single source of truth for all selection
 * Re-exported from SDK for backward compatibility
 */
export type UnifiedSelection = UnifiedSelectionState;

/**
 * Unified selection atom - directly uses the SDK unified state atom
 * This is now just an alias for the SDK atom
 */
export const unifiedSelectionAtom = atom(
  (get): UnifiedSelection => get(unifiedSelectionStateAtom),
  (get, set, update: UnifiedSelection | ((prev: UnifiedSelection) => UnifiedSelection)) => {
    const current = get(unifiedSelectionStateAtom);
    const next = typeof update === 'function' ? update(current) : update;
    const finalValue: UnifiedSelection = {
      ...current,
      ...next,
    };

    console.log('📝 Unified Selection Updated:', finalValue);
    set(unifiedSelectionStateAtom, finalValue);
  }
);

/**
 * Active tab state
 */
export type TabValue = 'preview' | 'curl' | 'graph';
export const activeTabAtom = atom<TabValue>('preview');

/**
 * Detail panel state (for graph view)
 */
export const detailPanelStateAtom = atom({
  isOpen: false,
});

// ============================================================================
// DERIVED STATE
// ============================================================================

/**
 * View mode - determines what UI to show based on current selection
 */
export const viewModeAtom = atom((get) => {
  const selection = get(unifiedSelectionAtom);
  const { selectedFn } = get(originalSelectionAtom);
  const activeWorkflow = get(activeWorkflowAtom);

  // Is the selected function part of a workflow?
  const isInWorkflow = selection.activeWorkflowId !== null;

  console.log(selection, selectedFn, activeWorkflow, isInWorkflow);
  const isLLMFunction = selectedFn?.type === 'llm_function';
  const hasWorkflowStructure =
    selectedFn?.type === 'workflow' && (selectedFn.nodes?.length ?? 0) > 1;
  const activeWorkflowHasStructure =
    activeWorkflow?.nodes && activeWorkflow.nodes.length > 1;
  // HACK: Treat single-node workflows as LLM-only so the UI doesn't expose the graph view
  // until we have richer structure from the runtime.
  const shouldShowWorkflow = hasWorkflowStructure;
  const showGraph =
    shouldShowWorkflow || (isInWorkflow && !!activeWorkflowHasStructure);

  return {
    showTabs: isLLMFunction, //|| showGraph,
    showGraphTab: showGraph,
    defaultTab: (showGraph ? 'graph' : 'preview') as TabValue,
    showTabBar: isLLMFunction || showGraph,
  };
});

/**
 * Bottom panel mode - determines whether to show TestPanel or DetailPanel
 */
export type BottomPanelMode = 'test-panel' | 'detail-panel';
export const bottomPanelModeAtom = atom<BottomPanelMode>((get) => {
  const activeTab = get(activeTabAtom);
  const selection = get(unifiedSelectionAtom);

  // Show DetailPanel when:
  // - On Graph tab, OR
  // - A graph node is selected (even if on other tabs)
  console.log('bottomPanelModeAtom', activeTab, selection);
  if (activeTab === 'graph' || selection.activeWorkflowId !== null) {
    return 'detail-panel';
  }

  // Show TestPanel for Preview/cURL tabs
  return 'test-panel';
});

/**
 * Helper atom to determine if we should show the graph view
 */
export const shouldShowGraphAtom = atom((get) => {
  const selection = get(unifiedSelectionAtom);
  const activeTab = get(activeTabAtom);

  // Show graph if we're on the graph tab or if we're in a workflow
  return activeTab === 'graph' && selection.activeWorkflowId !== null;
});
