/**
 * Centralized Mock Runtime Configuration
 *
 * All mock data in one place, strongly typed
 */

import type {
  WorkflowDefinition,
  BAMLFile,
} from '../types';
import type { FunctionWithCallGraph, TestCaseMetadata, PromptInfo } from '../interface';


/**
 * Node output generator function
 * Takes context and inputs, returns outputs
 */
export type NodeOutputGenerator = (
  context: Record<string, any>,
  inputs: Record<string, any>
) => Record<string, any>;

/**
 * Centralized mock runtime configuration
 * Now uses unified types from interface layer
 */
export interface MockRuntimeConfig {
  // Workflows are FunctionWithCallGraph with WorkflowDefinition compatibility
  workflows: FunctionWithCallGraph[];

  // Standalone functions (not in workflows)
  functions: FunctionWithCallGraph[];

  // Test cases organized by node ID
  testCases: Record<string, TestCaseMetadata[]>;

  // Node output generators (one per node or workflow.node)
  nodeOutputs: Record<string, NodeOutputGenerator>;

  // Execution behavior configuration
  executionBehavior: {
    cacheHitRate: number;
    errorRate: number;
    verboseLogging: boolean;
    speedMultiplier: number;
    nodeDelays: Record<string, () => number>;
    conditionalBranches?: Record<string, () => string>;
  };

  // BAML files (for debug panel)
  bamlFiles: BAMLFile[];

  // Prompt + cURL templates for LLM functions
  functionPrompts: Record<
    string,
    {
      prompt: PromptInfo;
      curl: {
        withSecrets: string;
        withoutSecrets: string;
      };
    }
  >;
}
