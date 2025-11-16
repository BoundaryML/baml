/**
 * Navigation Utilities
 *
 * Helper functions for navigation logic
 */

import type { FunctionWithCallGraph } from '../interface';
import type { GraphNode } from '../types';

/**
 * Extract which functions a node calls
 *
 * A node can:
 * - Call multiple functions: ### some node\n  callFunc1()\n  callFunc2()
 * - Be a function definition itself: ### functionA\n  functionA() -> .. { ... }
 *
 * Current limitation: WASM doesn't always expose this, so we try multiple methods:
 * 1. Check node.functionName (single function)
 * 2. Check metadata.functionCalls[] (future WASM enhancement)
 * 3. Parse ALL function calls from label
 * 4. Check if label exactly matches a function name
 *
 * Future: WASM should provide node.functionCalls[]
 */
export function extractCalledFunctions(
  node: GraphNode | undefined,
  allFunctions: FunctionWithCallGraph[]
): string[] {
  if (!node) return [];

  const functionSet = new Set<string>();

  // Method 1: Direct functionName property (single function)
  if (node.functionName) {
    functionSet.add(node.functionName);
  }

  // Method 2: Future WASM enhancement (if metadata contains function calls)
  if ((node as any).metadata?.functionCalls) {
    const calls = (node as any).metadata.functionCalls as Array<{ functionName: string }>;
    calls.forEach((call) => functionSet.add(call.functionName));
  }

  // Method 3: Parse ALL function calls from label
  // e.g., "if (CheckCondition(...))" -> ["CheckCondition"]
  // e.g., "callFunc1()\ncallFunc2()" -> ["callFunc1", "callFunc2"]
  if (node.label) {
    // Find all patterns like "FunctionName("
    const matches = node.label.matchAll(/(\w+)\s*\(/g);
    for (const match of matches) {
      const potentialFunction = match[1];
      // Verify it's actually a function
      if (potentialFunction && allFunctions.some((f) => f.name === potentialFunction)) {
        functionSet.add(potentialFunction);
      }
    }
  }

  // Method 4: Check if label exactly matches a function name
  if (node.label) {
    const func = allFunctions.find((f) => f.name === node.label);
    if (func) functionSet.add(func.name);
  }

  return Array.from(functionSet);
}

/**
 * Get the primary function from a list (convenience helper)
 */
export function getPrimaryFunction(functions: string[]): string | null {
  return functions[0] ?? null;
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
