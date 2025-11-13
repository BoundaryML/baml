/**
 * Navigation Utilities
 *
 * Helper functions for navigation logic
 */

import type { FunctionWithCallGraph } from '../interface';
import type { GraphNode } from '../types';

/**
 * Extract which function a node calls (if any)
 *
 * Current limitation: WASM doesn't always expose this, so we try multiple methods:
 * 1. Check node.functionName
 * 2. Parse from label (e.g., "if (CheckCondition(...))" -> "CheckCondition")
 * 3. Check if label exactly matches a function name
 *
 * Future: WASM should provide node.functionCalls[]
 */
export function extractCalledFunction(
  node: GraphNode | undefined,
  allFunctions: FunctionWithCallGraph[]
): string | null {
  if (!node) return null;

  // Method 1: Direct functionName property
  if (node.functionName) {
    return node.functionName;
  }

  // Method 2: Future WASM enhancement (if metadata contains function calls)
  if ((node as any).metadata?.functionCalls?.[0]) {
    return (node as any).metadata.functionCalls[0].functionName;
  }

  // Method 3: Parse from label
  // e.g., "if (CheckCondition(...))" -> "CheckCondition"
  // e.g., "call ValidateInput(...)" -> "ValidateInput"
  const match = node.label?.match(/(\w+)\s*\(/);
  if (match?.[1]) {
    const potentialFunction = match[1];
    // Verify it's actually a function
    if (allFunctions.some((f) => f.name === potentialFunction)) {
      return potentialFunction;
    }
  }

  // Method 4: Check if label exactly matches a function name
  const func = allFunctions.find((f) => f.name === node.label);
  if (func) return func.name;

  return null;
}

/**
 * Select the best test to show
 *
 * Priority:
 * 1. Currently selected test (if still valid)
 * 2. First available test
 * 3. null
 */
export function selectPreferredTest(
  availableTests: string[],
  currentTest: string | null
): string | null {
  if (!availableTests.length) return null;

  // Preserve current test if valid
  if (currentTest && availableTests.includes(currentTest)) {
    return currentTest;
  }

  // Otherwise pick first
  return availableTests[0] ?? null;
}

/**
 * Select the best workflow from memberships
 *
 * Priority:
 * 1. Current workflow (if function is in it)
 * 2. First workflow
 */
export function selectBestWorkflow(
  memberships: any[],
  currentState: { mode: string; workflowId?: string }
): any {
  if (currentState.mode === 'workflow' && currentState.workflowId) {
    const match = memberships.find((m) => m.workflowId === currentState.workflowId);
    if (match) return match;
  }

  return memberships[0];
}
