/**
 * Target Enricher
 *
 * Takes raw navigation input and enriches with full context
 */

import type { FunctionWithCallGraph } from '../interface';
import type { BAMLFile, BAMLTest } from '../types';
import type {
  NavigationInput,
  EnrichedTarget,
  NavigationContext,
  WorkflowMembership,
} from './types';
import { WorkflowIndex } from './workflow-index';
import { extractCalledFunctions } from './utils';

export class TargetEnricher {
  private workflowIndex: WorkflowIndex;
  private context: NavigationContext;

  constructor(context: NavigationContext) {
    this.context = context;
    this.workflowIndex = new WorkflowIndex(context.workflows);
  }

  /**
   * Update context (e.g., when workflows change)
   */
  updateContext(context: NavigationContext): void {
    this.context = context;
    this.workflowIndex.rebuild(context.workflows);
  }

  /**
   * Get current context (for use in coordinator)
   */
  getContext(): NavigationContext {
    return this.context;
  }

  /**
   * Enrich a navigation input with full context
   */
  enrich(input: NavigationInput): EnrichedTarget {
    // Determine the target name
    const name =
      input.functionName || input.testName || input.nodeId || '';

    // Build enriched target
    const target: EnrichedTarget = {
      name,
      kind: input.kind,
      exists: this.checkExists(input),
      workflowMemberships: this.findWorkflowUsages(input),
      availableTests: this.findTests(input),
      functionType: this.getFunctionType(input),
      functionName: input.functionName,
      testName: input.testName,
      workflowId: input.workflowId,
      nodeId: input.nodeId,
    };

    return target;
  }

  /**
   * Check if the target exists in the codebase
   */
  private checkExists(input: NavigationInput): boolean {
    if (input.kind === 'test') {
      return this.context.tests.some((t) => t.name === input.testName);
    }

    if (input.kind === 'function') {
      return this.context.functions.some((f) => f.name === input.functionName);
    }

    if (input.kind === 'node' && input.workflowId) {
      const workflow = this.context.workflows.find(
        (w) => w.id === input.workflowId
      );
      return Boolean(
        workflow?.nodes?.some((n) => n.id === input.nodeId)
      );
    }

    return false;
  }

  /**
   * Find all workflow usages of the target
   */
  private findWorkflowUsages(input: NavigationInput): WorkflowMembership[] {
    // If clicking a node directly, return that specific membership
    if (input.kind === 'node' && input.workflowId && input.nodeId) {
      const workflow = this.context.workflows.find(
        (w) => w.id === input.workflowId
      );
      if (workflow) {
        const node = workflow.nodes?.find((n) => n.id === input.nodeId);
        if (node) {
          const calledFunctions = extractCalledFunctions(node, this.context.workflows);
          return [
            {
              workflowId: workflow.id,
              nodeId: node.id,
              nodeLabel: node.label,
              calledFunctions,
            },
          ];
        }
      }
      return [];
    }

    // For function/test clicks, use the index to find memberships
    const targetName = input.functionName || input.testName || '';
    if (targetName) {
      return this.workflowIndex.lookup(targetName);
    }

    return [];
  }

  /**
   * Find all tests for the target
   */
  private findTests(input: NavigationInput): string[] {
    const tests: string[] = [];

    // Get the function name (either from input or from test or from node)
    let functionName = input.functionName;

    if (input.kind === 'test') {
      // For test clicks, find the function being tested
      const test = this.context.tests.find((t) => t.name === input.testName);
      functionName = test?.functionName;
    }

    if (input.kind === 'node' && input.workflowId && input.nodeId) {
      // For node clicks, extract function names from the node
      const workflow = this.context.workflows.find(
        (w) => w.id === input.workflowId
      );
      if (workflow) {
        const node = workflow.nodes?.find((n) => n.id === input.nodeId);
        if (node) {
          const calledFunctions = extractCalledFunctions(node, this.context.workflows);
          // Use the first called function to find tests
          functionName = calledFunctions[0] ?? workflow.id;
        }
      }
    }

    if (!functionName) return tests;

    // Find all tests for this function
    for (const file of this.context.bamlFiles) {
      for (const test of file.tests) {
        if (test.functionName === functionName) {
          tests.push(test.name);
        }
      }
    }

    return tests;
  }

  /**
   * Get the function type
   */
  private getFunctionType(input: NavigationInput): string | undefined {
    if (input.functionType) return input.functionType;

    if (input.functionName) {
      const func = this.context.functions.find(
        (f) => f.name === input.functionName
      );
      return func?.type;
    }

    return undefined;
  }
}
