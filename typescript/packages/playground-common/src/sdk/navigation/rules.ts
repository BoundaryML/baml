/**
 * Navigation Rules
 *
 * Explicit rules that determine navigation behavior
 */

import type { SelectionState } from '../atoms/core.atoms';
import type { NavigationRule, EnrichedTarget, NavigationInput } from './types';
import { selectPreferredTest, selectBestWorkflow, getPrimaryFunction } from './utils';

/**
 * Priority 0: Direct node selection from graph
 *
 * When clicking a node in the graph, select it directly without heuristics
 */
const directNodeClick: NavigationRule = {
  id: 'direct-node-click',
  priority: 0,
  matches: (target) => target.kind === 'node',
  resolve: (target, current) => {
    if (!target.workflowId || !target.nodeId) {
      throw new Error('Node click requires workflowId and nodeId');
    }

    const membership = target.workflowMemberships[0];

    // Preserve current test name when clicking nodes
    const currentTestName = (current.mode === 'workflow' || current.mode === 'function')
      ? current.testName
      : null;

    return {
      mode: 'workflow',
      workflowId: target.workflowId,
      selectedNodeId: target.nodeId,
      // Use function name from input, fallback to membership's called functions
      functionName: target.functionName ?? (membership ? getPrimaryFunction(membership.calledFunctions) : null),
      testName: selectPreferredTest(target.availableTests, currentTestName),
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
  resolve: (target, current, context) => {
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
        // Use the test's function name, not the node's called functions
        // The test belongs to a specific function, and that's what we need for sidebar highlighting
        functionName: target.functionName ?? getPrimaryFunction(membership.calledFunctions),
        testName: target.testName ?? null,
      };
    }

    // Check if the function itself IS a workflow (expr function)
    // This handles tests for workflow functions directly
    const functionName = target.functionName ?? target.name;


    // Check in workflows first
    let workflow = context?.workflows?.find(
      (w) => w.name === functionName && w.type === 'workflow'
    );

    // Also check in functions list - check for expr functions (workflows)
    // Only expr functions should be shown in workflow mode, not LLM functions
    if (!workflow) {
      workflow = context?.functions?.find(
        (f) => f.name === functionName && (f.type === 'workflow' || f.functionFlavor === 'expr')
      );
    }

    if (workflow) {
      // Find the root node ID
      const rootNode = workflow.nodes?.find((n) => n.id.includes('|root:'));
      return {
        mode: 'workflow',
        workflowId: workflow.id,
        selectedNodeId: rootNode?.id ?? `${workflow.id}|root:0`,
        functionName: workflow.name,
        testName: target.testName ?? null,
      };
    }

    // Otherwise, show function mode
    return {
      mode: 'function',
      functionName,
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
      functionName: getPrimaryFunction(membership.calledFunctions),
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
      functionName: getPrimaryFunction(membership.calledFunctions),
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
 * Priority 998: Loading state
 *
 * When target doesn't exist but we have intent, enter loading state
 */
const loadingState: NavigationRule = {
  id: 'loading-state',
  priority: 998,
  matches: (target) =>
    !target.exists &&
    (!!target.functionName || !!target.testName || !!target.workflowId),
  resolve: (target) => {
    // Preserve the original navigation input
    const intent: NavigationInput = {
      kind: target.kind,
      source: 'api',
      timestamp: Date.now(),
      functionName: target.functionName,
      testName: target.testName,
      workflowId: target.workflowId,
      nodeId: target.nodeId,
    };

    return {
      mode: 'loading',
      intent,
      startedAt: Date.now(),
    };
  },
  explain: (target) =>
    `Target "${target.name}" not found yet, entering loading state`,
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
  resolve: () => ({ mode: 'empty', reason: 'no-files' }),
  explain: () => 'No valid target, showing empty state',
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
  loadingState,
  emptyState,
];
