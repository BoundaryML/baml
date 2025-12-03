/**
 * State Manager
 *
 * Manages state changes and side effects for navigation
 */

import type { SelectionState } from '../atoms/core.atoms';
import type { SideEffect, JotaiSet, NavigationInput, NavigationContext } from './types';
import type { CallGraphNode } from '../interface';
import {
  unifiedSelectionStateAtom,
  detailPanelAtom,
  selectedInputSourceAtom,
} from '../atoms/core.atoms';
import { activeTabAtom } from '../../shared/baml-project-panel/playground-panel/unified-atoms';
import { panToNodeIfNeeded } from '../../utils/cameraPan';
import { flowStore } from '../../states/reactflow';
import { vscode } from '../../shared/baml-project-panel/vscode';

export class StateManager {
  /**
   * Build a transaction (state + side effects)
   *
   * Determines what side effects should occur based on the target state
   */
  buildTransaction(
    targetState: SelectionState,
    currentState: SelectionState,
    input?: NavigationInput,
    context?: NavigationContext
  ): SideEffect[] {
    const effects: SideEffect[] = [];

    if (targetState.mode === 'workflow') {
      effects.push({ type: 'switch-tab', tab: 'graph' });
      effects.push({ type: 'open-panel' });
      effects.push({
        type: 'pan-to-node',
        workflowId: targetState.workflowId,
        nodeId: targetState.selectedNodeId,
      });

      // Jump to file when clicking a different node from the graph
      if (
        input?.source === 'graph' &&
        input?.kind === 'node' &&
        currentState.mode === 'workflow' &&
        currentState.selectedNodeId !== targetState.selectedNodeId &&
        context
      ) {
        const span = this.findNodeSpan(
          targetState.workflowId,
          targetState.selectedNodeId,
          context
        );
        if (span) {
          effects.push({
            type: 'jump-to-file',
            span: {
              filePath: span.filePath,
              startLine: span.startLine,
              startColumn: span.startColumn,
            },
          });
        }
      }

      if (targetState.testName) {
        effects.push({ type: 'select-test', testName: targetState.testName });
      } else {
        effects.push({ type: 'clear-test' });
      }
    } else if (targetState.mode === 'function') {
      effects.push({ type: 'switch-tab', tab: 'preview' });
      effects.push({ type: 'open-panel' });

      if (targetState.testName) {
        effects.push({ type: 'select-test', testName: targetState.testName });
      } else {
        effects.push({ type: 'clear-test' });
      }
    } else {
      // Empty state
      effects.push({ type: 'switch-tab', tab: 'preview' });
      effects.push({ type: 'close-panel' });
      effects.push({ type: 'clear-test' });
    }

    return effects;
  }

  /**
   * Apply transaction atomically
   *
   * Updates all atoms and triggers side effects
   */
  async apply(
    state: SelectionState,
    effects: SideEffect[],
    atomSet: JotaiSet
  ): Promise<void> {
    // 1. Update selection atom (most important - do this first)
    console.log('looking: applying state', state);
    atomSet(unifiedSelectionStateAtom, state);

    // 2. Apply side effects
    for (const effect of effects) {
      switch (effect.type) {
        case 'switch-tab':
          atomSet(activeTabAtom, effect.tab);
          break;

        case 'open-panel':
          atomSet(detailPanelAtom, (prev: any) => ({ ...prev, isOpen: true }));
          break;

        case 'close-panel':
          atomSet(detailPanelAtom, (prev: any) => ({ ...prev, isOpen: false }));
          break;

        case 'select-test':
          atomSet(selectedInputSourceAtom, { testName: effect.testName } as any);
          break;

        case 'clear-test':
          atomSet(selectedInputSourceAtom, null);
          break;

        case 'pan-to-node':
          // Pan to node is handled by the graph component
          // It listens to selection changes and pans automatically
          break;

        case 'jump-to-file':
          await vscode.jumpToFile({
            filePath: effect.span.filePath,
            start: 0, // Character offset not needed for jump
            end: 0,
            startLine: effect.span.startLine,
            startColumn: effect.span.startColumn,
            endLine: effect.span.startLine,
            endColumn: effect.span.startColumn,
          });
          break;
      }
    }
  }

  /**
   * Find the span for a node in a workflow
   */
  private findNodeSpan(
    workflowId: string,
    nodeId: string,
    context: NavigationContext
  ): { filePath: string; startLine: number; startColumn: number } | null {
    // Find the workflow
    const workflow = context.workflows.find((w) => w.id === workflowId);
    if (!workflow?.callGraph) {
      return null;
    }

    // Recursively search for the node in the call graph
    const findNode = (node: CallGraphNode): CallGraphNode | null => {
      if (node.id === nodeId) {
        return node;
      }
      for (const child of node.children) {
        const found = findNode(child);
        if (found) {
          return found;
        }
      }
      return null;
    };

    const node = findNode(workflow.callGraph);
    if (node?.span) {
      return {
        filePath: node.span.filePath,
        startLine: node.span.startLine,
        startColumn: node.span.startColumn,
      };
    }

    return null;
  }
}
