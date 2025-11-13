/**
 * Navigation Rules
 *
 * Explicit rules that determine navigation behavior
 */

import type { SelectionState } from '../atoms/core.atoms';
import type { NavigationRule, EnrichedTarget } from './types';
import { selectPreferredTest, selectBestWorkflow } from './utils';

/**
 * Priority 0: Direct node selection from graph
 *
 * When clicking a node in the graph, select it directly without heuristics
 */
const directNodeClick: NavigationRule = {
  id: 'direct-node-click',
  priority: 0,
  matches: (target) => target.kind === 'node',
  resolve: (target) => {
    if (!target.workflowId || !target.nodeId) {
      throw new Error('Node click requires workflowId and nodeId');
    }

    const membership = target.workflowMemberships[0];

    return {
      mode: 'workflow',
      workflowId: target.workflowId,
      selectedNodeId: target.nodeId,
      testName: selectPreferredTest(target.availableTests, null),
    };
  },
  explain: (target) =>
    `Direct node click: ${target.workflowId} -> ${target.nodeId}`,
};

/**
 * Priority 1: Test selection
 *
 * When clicking a test, determine the best way to show it
 */
const testClick: NavigationRule = {
  id: 'test-click',
  priority: 1,
  matches: (target) => target.kind === 'test',
  resolve: (target, current) => {
    // If function is in a workflow, show workflow mode
    if (target.workflowMemberships.length > 0) {
      const membership = selectBestWorkflow(
        target.workflowMemberships,
        current
      );
      if (!membership) {
        throw new Error('Expected membership to be defined');
      }
      return {
        mode: 'workflow',
        workflowId: membership.workflowId,
        selectedNodeId: membership.nodeId,
        testName: target.testName ?? null,
      };
    }

    // Otherwise, show function mode
    return {
      mode: 'function',
      functionName: target.functionName ?? target.name,
      testName: target.testName ?? null,
    };
  },
  explain: (target) =>
    target.workflowMemberships.length > 0
      ? `Test targets function in workflow: ${target.workflowMemberships[0]?.workflowId || 'unknown'}`
      : `Test targets standalone function: ${target.functionName}`,
};

/**
 * Priority 2: Context preservation (stay in current workflow)
 *
 * If clicking a function that exists in the current workflow, stay there
 */
const stayInWorkflow: NavigationRule = {
  id: 'stay-in-workflow',
  priority: 2,
  matches: (target, current) =>
    target.kind === 'function' &&
    current.mode === 'workflow' &&
    target.workflowMemberships.some((m) => m.workflowId === current.workflowId),
  resolve: (target, current) => {
    if (current.mode !== 'workflow') {
      throw new Error('Expected workflow mode');
    }

    const membership = target.workflowMemberships.find(
      (m) => m.workflowId === current.workflowId
    );
    if (!membership) {
      throw new Error(`Expected to find node in workflow ${current.workflowId}`);
    }

    return {
      mode: 'workflow',
      workflowId: current.workflowId,
      selectedNodeId: membership.nodeId,
      testName: selectPreferredTest(target.availableTests, current.testName),
    };
  },
  explain: (target, current) =>
    current.mode === 'workflow'
      ? `Staying in ${current.workflowId} because ${target.name} is a node there`
      : '',
};

/**
 * Priority 3: Workflow discovery
 *
 * If clicking a function that's in a workflow, switch to that workflow
 */
const switchToWorkflow: NavigationRule = {
  id: 'switch-to-workflow',
  priority: 3,
  matches: (target) =>
    target.kind === 'function' && target.workflowMemberships.length > 0,
  resolve: (target) => {
    const membership = target.workflowMemberships[0]; // Pick first workflow
    if (!membership) {
      throw new Error('Expected at least one workflow membership');
    }

    return {
      mode: 'workflow',
      workflowId: membership.workflowId,
      selectedNodeId: membership.nodeId,
      testName: selectPreferredTest(target.availableTests, null),
    };
  },
  explain: (target) =>
    `Switching to workflow ${target.workflowMemberships[0]?.workflowId || 'unknown'}`,
};

/**
 * Priority 4: Function isolation
 *
 * If clicking a standalone function, show it in function mode
 */
const showFunction: NavigationRule = {
  id: 'show-function',
  priority: 4,
  matches: (target) => target.kind === 'function' && target.exists,
  resolve: (target) => ({
    mode: 'function',
    functionName: target.name,
    testName: selectPreferredTest(target.availableTests, null),
  }),
  explain: (target) => `Showing standalone function: ${target.name}`,
};

/**
 * Priority 999: Catch-all (empty state)
 *
 * If nothing else matches, show empty state
 */
const emptyState: NavigationRule = {
  id: 'empty-state',
  priority: 999,
  matches: () => true,
  resolve: () => ({ mode: 'empty' }),
  explain: () => 'Target not found, showing empty state',
};

/**
 * All navigation rules, in priority order
 */
export const NAVIGATION_RULES: NavigationRule[] = [
  directNodeClick,
  testClick,
  stayInWorkflow,
  switchToWorkflow,
  showFunction,
  emptyState,
];
