/**
 * Navigation Heuristic for Code Click Events
 *
 * This module implements the logic for determining what to display in the UI
 * when a user clicks on a function or test in their IDE (simulated via debug panel).
 *
 * ## Algorithm Overview
 *
 * The heuristic follows a priority-based decision tree:
 *
 * ### 1. Test Click
 * - Find the workflow being tested
 * - If workflow exists: Switch to that workflow
 * - Otherwise: Show empty state (test function is not a workflow)
 *
 * ### 2. Function Click
 *
 * **Priority 1: Stay in current workflow if possible**
 * - If function exists as a node in current workflow:
 *   → Select that node and pan to it
 *
 * **Priority 2: Find another workflow containing this function**
 * - Search all workflows for one containing this function
 * - If found: Switch to that workflow and select the node
 *
 * **Priority 3: Show function in isolation (if it has tests)**
 * - If function has test cases but is not part of any workflow
 * - Create a minimal graph showing just this function
 * - This is useful for testing individual functions
 *
 * **Priority 4: Empty state**
 * - Function is not part of any workflow
 * - Function has no tests
 * - Show helpful message suggesting the user create a workflow or test
 *
 * ## State Management
 *
 * The heuristic is stateful in the sense that it remembers:
 * - Current active workflow (to prioritize staying in context)
 * - Last successful workflow selection (for better UX)
 *
 * ## Example Scenarios
 *
 * **Scenario 1: User clicks test for a workflow**
 * ```
 * Click: test_simple_success (tests simpleWorkflow)
 * Result: Switch to simpleWorkflow graph
 * ```
 *
 * **Scenario 2: User clicks subfunction in current workflow**
 * ```
 * Current: simpleWorkflow graph
 * Click: processData (function in simpleWorkflow)
 * Result: Select processData node, pan to it
 * ```
 *
 * **Scenario 3: User clicks function in different workflow**
 * ```
 * Current: simpleWorkflow graph
 * Click: handleSuccess (function in conditionalWorkflow)
 * Result: Switch to conditionalWorkflow, select handleSuccess node
 * ```
 *
 * **Scenario 4: User clicks standalone function with tests**
 * ```
 * Click: extractUser (not in any workflow, but has tests)
 * Result: Show minimal graph for extractUser with its test cases
 * ```
 *
 * **Scenario 5: User clicks function with no context**
 * ```
 * Click: helperFunction (not in any workflow, no tests)
 * Result: Empty state with helpful message
 * ```
 */

import type { CodeClickEvent, BAMLFile } from './types';
import type { FunctionWithCallGraph } from './interface';
import type { SelectionState } from './atoms/core.atoms';

/**
 * Current navigation state passed to the heuristic
 */
export interface NavigationContext {
  /** Currently active workflow (if any) */
  activeWorkflowId: string | null;
  /** All available workflows */
  workflows: FunctionWithCallGraph[];
  /** All BAML files with functions and tests */
  bamlFiles: BAMLFile[];
}

/**
 * Main navigation heuristic function
 *
 * @param event - The code click event from the IDE/debug panel
 * @param state - Current navigation state
 * @returns Selection state to apply
 */
export function determineNavigationAction(
  event: CodeClickEvent,
  state: NavigationContext
): SelectionState {
  console.log('🧭 Navigation Heuristic:', { event, state });

  // Handle TEST clicks
  if (event.type === 'test') {
    return handleTestClick(event, state);
  }

  // Handle FUNCTION clicks
  return handleFunctionClick(event, state);
}

/**
 * Handle clicks on test cases
 */
function handleTestClick(
  event: CodeClickEvent & { type: 'test' },
  state: NavigationContext
): SelectionState {
  const targetFunction = event.functionName;

  // Check if target is a workflow itself
  const workflow = state.workflows.find(w => w.id === targetFunction);

  if (isWorkflowWithStructure(workflow)) {
    // Test is for a workflow - just switch to it
    console.log('✅ Test targets workflow:', workflow.id);
    return {
      mode: 'workflow',
      workflowId: workflow.id,
      selectedNodeId: workflow.id,
      testName: event.testName,
    };
  }

  // Priority 1: Check if function exists in current workflow (stay in context)
  if (state.activeWorkflowId) {
    const currentWorkflow = state.workflows.find(w => w.id === state.activeWorkflowId);
    if (isWorkflowWithStructure(currentWorkflow) && functionExistsInWorkflow(targetFunction, currentWorkflow)) {
      console.log('✅ Test targets function in current workflow, selecting node:', targetFunction, 'test:', event.testName);
      return {
        mode: 'workflow',
        workflowId: currentWorkflow.id,
        selectedNodeId: targetFunction,
        testName: event.testName,
      };
    }
  }

  // Priority 2: Check if target is a node within a different workflow
  const workflowWithFunction = findWorkflowContaining(targetFunction, state.workflows);

  if (isWorkflowWithStructure(workflowWithFunction)) {
    // Test is for a function node in a different workflow - switch and select
    console.log('✅ Test targets function in different workflow:', workflowWithFunction.id, '->', targetFunction, 'test:', event.testName);
    return {
      mode: 'workflow',
      workflowId: workflowWithFunction.id,
      selectedNodeId: targetFunction,
      testName: event.testName,
    };
  }

  // Priority 3: Test is for a standalone function - show its tests
  const tests = findTestsForFunction(targetFunction, state.bamlFiles);
  if (tests.length > 0) {
    console.log('✅ Test targets standalone function with tests:', targetFunction);
    return {
      mode: 'function',
      functionName: targetFunction,
      testName: event.testName,
    };
  }

  // Priority 4: Test function is not found anywhere
  console.log('⚠️ Test function not found in any workflow:', targetFunction);
  return { mode: 'empty' };
}

/**
 * Handle clicks on functions
 */
function handleFunctionClick(
  event: CodeClickEvent & { type: 'function' },
  state: NavigationContext
): SelectionState {
  const targetFunction = event.functionName;

  // Priority 1: Check if function exists in current workflow
  if (state.activeWorkflowId) {
    const currentWorkflow = state.workflows.find(w => w.id === state.activeWorkflowId);
    if (functionExistsInWorkflow(targetFunction, currentWorkflow)) {
      console.log('✅ Function exists in current workflow, selecting node');
      return {
        mode: 'workflow',
        workflowId: currentWorkflow!.id,
        selectedNodeId: targetFunction,
        testName: null,
      };
    }
  }

  // Priority 2: Find another workflow containing this function
  const workflowWithFunction = findWorkflowContaining(targetFunction, state.workflows);
  if (workflowWithFunction) {
    console.log('✅ Found function in different workflow:', workflowWithFunction.id);
    return {
      mode: 'workflow',
      workflowId: workflowWithFunction.id,
      selectedNodeId: targetFunction,
      testName: null,
    };
  }

  // Priority 3: Check if function has tests (show in isolation)
  const tests = findTestsForFunction(targetFunction, state.bamlFiles);
  if (tests.length > 0) {
    console.log('✅ Function has tests, showing in isolation');
    return {
      mode: 'function',
      functionName: targetFunction,
      testName: tests[0] ?? null,
    };
  }

  // Priority 4: Empty state
  console.log('⚠️ Function not found in any context');
  return { mode: 'empty' };
}

/**
 * Check if a function exists as a node in a workflow
 * TODO: we don't know currently if an llm function exists in a workflow.
 *  we need to surface this info.
 */
function functionExistsInWorkflow(
  functionName: string,
  workflow?: FunctionWithCallGraph | null
): workflow is FunctionWithCallGraph {
  return Boolean(workflow && workflow.nodes?.some(node => node.functionName === functionName));
}

/**
 * Find a workflow that contains the given function as a node
 * Note: workflows array should only contain multi-node workflows (not standalone functions)
 */
function findWorkflowContaining(
  functionName: string,
  workflows: FunctionWithCallGraph[]
): FunctionWithCallGraph | null {
  return workflows.find(workflow =>
    functionExistsInWorkflow(functionName, workflow)
  ) ?? null;
}

/**
 * Find all tests that test the given function
 */
function findTestsForFunction(
  functionName: string,
  bamlFiles: BAMLFile[]
): string[] {
  const tests: string[] = [];

  for (const file of bamlFiles) {
    for (const test of file.tests) {
      // Check if this test belongs to the specified function
      if (test.functionName === functionName) {
        tests.push(test.name);
      }
    }
  }

  return tests;
}

/**
 * Helper to get current navigation state from SDK
 */
export function getCurrentNavigationState(
  sdk: {
    workflows: { getAll: () => FunctionWithCallGraph[], getActive: () => FunctionWithCallGraph | null },
    diagnostics: { getBAMLFiles: () => BAMLFile[], getFunctions: () => FunctionWithCallGraph[] }
  }
): NavigationContext {
  const activeWorkflow = sdk.workflows.getActive();
  return {
    activeWorkflowId: isWorkflowWithStructure(activeWorkflow) ? activeWorkflow!.id : null,
    // Use getFunctions() to get ALL functions (for checking workflow membership)
    workflows: sdk.diagnostics.getFunctions(),
    bamlFiles: sdk.diagnostics.getBAMLFiles(),
  };
}

export function isWorkflowWithStructure(workflow?: FunctionWithCallGraph | null): workflow is FunctionWithCallGraph {
  return Boolean(workflow && workflow.type === 'workflow' && (workflow.nodes?.length ?? 0) > 1);
}
