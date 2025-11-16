/**
 * Workflow Index for Fast Lookups
 *
 * Builds an index mapping functions to their workflow memberships.
 * This provides O(1) lookup time instead of O(workflows × nodes).
 */

import type { FunctionWithCallGraph } from '../interface';
import type { WorkflowMembership } from './types';
import { extractCalledFunctions } from './utils';

export class WorkflowIndex {
  private functionToWorkflows = new Map<string, WorkflowMembership[]>();

  constructor(workflows: FunctionWithCallGraph[]) {
    this.rebuild(workflows);
  }

  /**
   * Rebuild the index from scratch
   */
  rebuild(workflows: FunctionWithCallGraph[]): void {
    this.functionToWorkflows.clear();

    for (const workflow of workflows) {
      // Skip non-workflows or single-node workflows
      if (workflow.type !== 'workflow' || (workflow.nodes?.length ?? 0) <= 1) {
        continue;
      }

      for (const node of workflow.nodes) {
        // Get ALL functions called by this node
        const calledFunctions = extractCalledFunctions(node, workflows);

        const membership: WorkflowMembership = {
          workflowId: workflow.id,
          nodeId: node.id,
          nodeLabel: node.label,
          calledFunctions,
        };

        // Index by node ID (always)
        this.addMembership(node.id, membership);

        // Also index by function name (if different from node ID)
        if (node.functionName && node.functionName !== node.id) {
          this.addMembership(node.functionName, membership);
        }

        // Also index by EACH called function
        for (const calledFunction of calledFunctions) {
          this.addMembership(calledFunction, membership);
        }
      }
    }
  }

  /**
   * Look up workflow memberships by function name
   */
  lookup(functionName: string): WorkflowMembership[] {
    return this.functionToWorkflows.get(functionName) || [];
  }

  /**
   * Add a membership to the index
   */
  private addMembership(key: string, membership: WorkflowMembership): void {
    const memberships = this.functionToWorkflows.get(key) || [];
    memberships.push(membership);
    this.functionToWorkflows.set(key, memberships);
  }

  /**
   * Get all indexed functions
   */
  getAllFunctions(): string[] {
    return Array.from(this.functionToWorkflows.keys());
  }

  /**
   * Check if a function exists in any workflow
   */
  hasFunction(functionName: string): boolean {
    return this.functionToWorkflows.has(functionName);
  }
}
