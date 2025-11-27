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
  type SelectionState,
  activeWorkflowAtom,
} from '../../../sdk/atoms/core.atoms';

// ============================================================================
// CORE UNIFIED STATE
// ============================================================================

/**
 * Unified selection state - single source of truth for all selection
 * Re-exported from SDK for backward compatibility
 */
export type UnifiedSelection = SelectionState;

/**
 * Unified selection atom - directly uses the SDK unified state atom
 * This is now just an alias for the SDK atom
 */
export const unifiedSelectionAtom = atom(
  (get): UnifiedSelection => get(unifiedSelectionStateAtom),
  (get, set, update: UnifiedSelection | ((prev: UnifiedSelection) => UnifiedSelection)) => {
    const current = get(unifiedSelectionStateAtom);
    const next = typeof update === 'function' ? update(current) : update;

    console.log('📝 Unified Selection Updated:', next);
    set(unifiedSelectionStateAtom, next);
  }
);

/**
 * Active tab state
 */
export type TabValue = 'preview' | 'curl' | 'graph';
export const activeTabAtom = atom<TabValue>('preview');

// ============================================================================
// DERIVED STATE
// ============================================================================

/**
 * View mode - determines what UI to show based on current selection
 */
export const viewModeAtom = atom((get) => {
  const selection = get(unifiedSelectionAtom);
  const { selectedFn } = get(originalSelectionAtom);

  // Is the selected function part of a workflow?
  const isInWorkflow = selection.mode === 'workflow';

  const isLLMFunction = selectedFn?.functionFlavor === 'llm';
  const isExprFunction = selectedFn?.functionFlavor === 'expr' || selectedFn?.type === 'workflow';

  // Show graph for expr functions (workflows) - they always have a graph view
  const showGraph = isExprFunction || isInWorkflow;

  return {
    showGraphTab: showGraph,
    showLLMTabs: isLLMFunction, // Show Preview/cURL tabs only for LLM functions
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
  const { selectedFn } = get(originalSelectionAtom);

  // Show TestPanel when:
  // - NOT in workflow mode AND
  // - It's an LLM function
  const isLLMFunction = selectedFn?.functionFlavor === 'llm';
  if (selection.mode !== 'workflow' && isLLMFunction) {
    return 'test-panel';
  }

  // Show DetailPanel when:
  // - On Graph tab, OR
  // - A workflow is active, OR
  // - In function mode (non-LLM functions)
  console.log('bottomPanelModeAtom', activeTab, selection);
  if (
    activeTab === 'graph' ||
    selection.mode === 'workflow' ||
    selection.mode === 'function'
  ) {
    return 'detail-panel';
  }

  // Show TestPanel for empty mode
  return 'test-panel';
});

/**
 * Helper atom to determine if we should show the graph view
 */
export const shouldShowGraphAtom = atom((get) => {
  const selection = get(unifiedSelectionAtom);
  const activeTab = get(activeTabAtom);

  // Show graph if we're on the graph tab and in workflow mode
  return activeTab === 'graph' && selection.mode === 'workflow';
});
