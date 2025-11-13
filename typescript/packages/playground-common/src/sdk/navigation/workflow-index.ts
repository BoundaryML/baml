/**
 * Workflow Index for Fast Lookups
 *
 * Builds an index mapping functions to their workflow memberships.
 * This provides O(1) lookup time instead of O(workflows × nodes).
 */

import type { FunctionWithCallGraph } from '../interface';
import type { WorkflowMembership } from './types';
import { extractCalledFunction } from './utils';

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
        // Get the function called by this node
        const calledFunction = extractCalledFunction(node, workflows);

        // Index by node ID (always)
        this.addMembership(node.id, {
          workflowId: workflow.id,
          nodeId: node.id,
          nodeLabel: node.label,
          calledFunction,
        });

        // Also index by function name (if different from node ID)
        if (node.functionName && node.functionName !== node.id) {
          this.addMembership(node.functionName, {
            workflowId: workflow.id,
            nodeId: node.id,
            nodeLabel: node.label,
            calledFunction,
          });
        }

        // Also index by called function (if any)
        if (calledFunction) {
          this.addMembership(calledFunction, {
            workflowId: workflow.id,
            nodeId: node.id,
            nodeLabel: node.label,
            calledFunction,
          });
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
