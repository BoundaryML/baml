/**
 * State Manager
 *
 * Manages state changes and side effects for navigation
 */

import type { SelectionState } from '../atoms/core.atoms';
import type { SideEffect, JotaiSet } from './types';
import {
  unifiedSelectionStateAtom,
  detailPanelAtom,
  selectedInputSourceAtom,
} from '../atoms/core.atoms';
import { activeTabAtom } from '../../shared/baml-project-panel/playground-panel/unified-atoms';

export class StateManager {
  /**
   * Build a transaction (state + side effects)
   *
   * Determines what side effects should occur based on the target state
   */
  buildTransaction(
    targetState: SelectionState,
    _currentState: SelectionState
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
          await this.panToNode(effect.workflowId, effect.nodeId);
          break;
      }
    }
  }

  /**
   * Pan to a node in the graph
   *
   * This is async because we need to wait for the graph to render
   */
  private async panToNode(
    _workflowId: string,
    _nodeId: string
  ): Promise<void> {
    // The graph component will handle this automatically
    // when it sees the selectedNodeId change
    //
    // If we need explicit panning, we can use flowStore here:
    // const node = flowStore.value.getNodes?.().find(n => n.id === nodeId);
    // if (node) {
    //   flowStore.value.setCenter?.(node.position.x, node.position.y, {
    //     zoom: 1,
    //     duration: 300
    //   });
    // }
  }
}
