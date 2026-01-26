/**
 * MockBamlRuntime - In-memory mock implementation
 *
 * Uses centralized mock configuration (static, doesn't change with files)
 */

import type { TestCaseInput, BAMLFile } from '../types';
import type {
  BamlRuntimeInterface,
  ExecutionOptions,
  CursorPosition,
  CursorNavigationResult,
} from './BamlRuntimeInterface';
import type { MockRuntimeConfig } from '../mock-config/types';
import type { DiagnosticError, GeneratedFile } from '../atoms/core.atoms';
import { simulateExecution } from './simulator';

// Import unified types from interface layer
import {
  createMockPrompt,
  type FunctionWithCallGraph,
  type TestCaseMetadata,
  type CallGraphNode,
  type PromptInfo,
  type RichExecutionEvent,
  type TestExecutionContext,
} from '../interface';

export class MockBamlRuntime implements BamlRuntimeInterface {
  private config: MockRuntimeConfig;
  private executionCount = 0;

  private constructor(config: MockRuntimeConfig) {
    this.config = config;
  }

  /**
   * Factory method to create new runtime instance
   * Matches wasmAtom pattern: WasmProject.new('./', bamlFiles)
   */
  static async create(
    files: Record<string, string>,
    config: MockRuntimeConfig
  ): Promise<MockBamlRuntime> {
    console.log(
      'Creating new MockBamlRuntime with',
      Object.keys(files).length,
      'files'
    );
    // Mock: We ignore files and use static config
    // In a real runtime, this would parse/compile the files
    return new MockBamlRuntime(config);
  }

  getVersion(): string {
    return '0.0.0-mock';
  }

  getWasmRuntime(): undefined {
    return undefined;
  }

  getWorkflows(): FunctionWithCallGraph[] {
    // Workflows are FunctionWithCallGraph objects with workflow compatibility fields (nodes, edges, etc.)
    return this.config.workflows;
  }

  getCallGraph(functionName: string): CallGraphNode | undefined {
    const functions = this.getFunctions();
    const func = functions.find(f => f.name === functionName);
    return func?.callGraph;
  }

  getFunctions(): FunctionWithCallGraph[] {
    // Collect all functions from BAML files
    const allFunctions: FunctionWithCallGraph[] = [];

    for (const file of this.config.bamlFiles) {
      allFunctions.push(...file.functions);
    }

    // Also include any standalone functions from config
    if (this.config.functions.length > 0) {
      allFunctions.push(...this.config.functions);
    }

    return allFunctions;
  }

  getTestCases(functionName?: string): TestCaseMetadata[] {
    if (!functionName) {
      return Object.values(this.config.testCases).flat();
    }
    return this.config.testCases[functionName] || [];
  }

  getBAMLFiles(): BAMLFile[] {
    return this.config.bamlFiles;
  }

  getDiagnostics(): DiagnosticError[] {
    // Mock runtime has no diagnostics (always valid)
    return [];
  }

  getGeneratedFiles(): GeneratedFile[] {
    // Mock runtime doesn't generate files
    return [];
  }

  async *executeWorkflow(
    workflowId: string,
    inputs: Record<string, any>,
    options?: ExecutionOptions
  ): AsyncGenerator<RichExecutionEvent> {
    const workflow = this.config.workflows.find((w) => w.id === workflowId);
    if (!workflow) {
      throw new Error(`Workflow ${workflowId} not found`);
    }

    this.executionCount++;
    const executionId = `exec_${Date.now()}_${this.executionCount}`;

    // Use centralized execution simulator
    yield* simulateExecution(
      workflow,
      this.config,
      inputs,
      executionId,
      options?.startFromNodeId
    );
  }

  async executeTest(
    functionName: string,
    testName: string,
    context: TestExecutionContext
  ): Promise<void> {
    await this.executeTests([{ functionName, testName }], context);
  }

  async executeTests(
    tests: Array<{ functionName: string; testName: string }>,
    context: TestExecutionContext
  ): Promise<void> {
    // Mock implementation - just simulate execution
    for (const test of tests) {
      const t = this.getTestCases(test.functionName).find((tc) => tc.name === test.testName);
      if (!t) throw new Error(`Test ${test.testName} not found for function ${test.functionName}`);

      // Simulate partial response callback
      if (context.onPartialResponse) {
        await new Promise(resolve => setTimeout(resolve, 100));
        context.onPartialResponse(test.functionName, test.testName, {
          parsed_response: { value: 'Mock response', checkCount: 0 },
        });
      }

      // Simulate completion
      await new Promise(resolve => setTimeout(resolve, 200));
    }
  }

  async cancelExecution(executionId: string): Promise<void> {
    console.log(`Cancelling execution: ${executionId}`);
  }

  async renderPromptForTest(
    functionName: string,
    testName: string,
    context: TestExecutionContext
  ): Promise<PromptInfo> {
    const template = this.config.functionPrompts?.[functionName];
    if (template) {
      return template.prompt;
    }
    return createMockPrompt('chat', 'mock-client');
  }

  async renderCurlForTest(
    functionName: string,
    testName: string,
    options: {
      stream: boolean;
      expandImages: boolean;
      exposeSecrets: boolean;
    },
    context: TestExecutionContext
  ): Promise<string> {
    const template = this.config.functionPrompts?.[functionName];
    if (template) {
      return options?.exposeSecrets
        ? template.curl.withSecrets
        : template.curl.withoutSecrets;
    }
    return `curl -X POST https://mock-api.example.com/${functionName} -d '{"test": "${testName}"}'`;
  }

  updateCursor(
    _cursor: CursorPosition,
    _fileContents: Record<string, string>,
    _currentSelection: string | null
  ): CursorNavigationResult {
    // Mock implementation - could be enhanced to parse the file and find functions/tests
    // For now, just return null (no navigation)
    return { functionName: null, testCaseName: null, nodeId: null };
  }
}
