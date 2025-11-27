/**
 * Navigation Rules Tests
 */

import { describe, it, expect } from 'vitest';
import { RuleEngine } from './rule-engine';
import { NAVIGATION_RULES } from './rules';
import type { EnrichedTarget, NavigationContext } from './types';
import type { SelectionState } from '../atoms/core.atoms';
import type { FunctionWithCallGraph } from '../interface';

describe('Navigation Rules', () => {
  const ruleEngine = new RuleEngine(NAVIGATION_RULES);

  describe('direct-node-click rule', () => {
    it('should preserve testName when clicking the same workflow node twice', () => {
      // Setup: User is in a workflow with a test selected (NOT the first one in the list)
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'test_simple_failure', // <-- Second test is selected
      };

      // Action: User clicks on the same root node again (double-click scenario)
      const target: EnrichedTarget = {
        name: 'SimpleWorkflow',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|root:0',
          nodeLabel: 'SimpleWorkflow',
          calledFunctions: ['SimpleWorkflow'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|root:0',
      };

      const result = ruleEngine.decide(target, currentState);

      // The testName should be preserved, not reset to the first available test
      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.testName).toBe('test_simple_failure');
      }
    });

    it('should preserve testName when clicking a different node in the same workflow', () => {
      // Setup: User is in a workflow with a test selected (NOT the first in the list)
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'test_simple_failure', // <-- Second test is selected
      };

      // Action: User clicks a different node in the same workflow
      const target: EnrichedTarget = {
        name: 'ProcessStep',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|step1:0',
          nodeLabel: 'ProcessStep',
          calledFunctions: ['ProcessStep'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|step1:0',
      };

      const result = ruleEngine.decide(target, currentState);

      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.testName).toBe('test_simple_failure');
      }
    });

    it('should select first available test when no test is currently selected', () => {
      // Setup: User is in a workflow with no test selected
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: null,
      };

      // Action: User clicks a node
      const target: EnrichedTarget = {
        name: 'SimpleWorkflow',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|root:0',
          nodeLabel: 'SimpleWorkflow',
          calledFunctions: ['SimpleWorkflow'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|root:0',
      };

      const result = ruleEngine.decide(target, currentState);

      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.testName).toBe('test_simple_success');
      }
    });

    it('should select first test when current test is not in available tests', () => {
      // Setup: User is in a workflow with a test that doesn't exist for this node
      const currentState: SelectionState = {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'nonexistent_test',
      };

      // Action: User clicks a node with different available tests
      const target: EnrichedTarget = {
        name: 'SimpleWorkflow',
        kind: 'node',
        exists: true,
        workflowMemberships: [{
          workflowId: 'SimpleWorkflow',
          nodeId: 'SimpleWorkflow|root:0',
          nodeLabel: 'SimpleWorkflow',
          calledFunctions: ['SimpleWorkflow'],
        }],
        availableTests: ['test_simple_success', 'test_simple_failure'],
        workflowId: 'SimpleWorkflow',
        nodeId: 'SimpleWorkflow|root:0',
      };

      const result = ruleEngine.decide(target, currentState);

      expect(result.rule).toBe('direct-node-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        // Should fall back to first available test
        expect(result.state.testName).toBe('test_simple_success');
      }
    });
  });

  describe('test-click rule', () => {
    // Helper to create a mock function
    const createMockFunction = (
      name: string,
      type: 'function' | 'llm_function' | 'workflow',
      flavor: 'llm' | 'expr'
    ): FunctionWithCallGraph => ({
      name,
      type,
      functionFlavor: flavor,
      span: { filePath: 'test.baml', start: 0, end: 100, startLine: 1, startColumn: 0, endLine: 10, endColumn: 1 },
      signature: `function ${name}()`,
      testSnippet: '',
      testCases: [],
      callGraph: { id: name, type: 'function', children: [] },
      isRoot: true,
      callGraphDepth: 1,
      id: name,
      displayName: name,
      filePath: 'test.baml',
      startLine: 1,
      endLine: 10,
      nodes: [{ id: `${name}|root:0`, type: 'function', label: name, codeHash: '', lastModified: Date.now() }],
      edges: [],
      entryPoint: name,
      parameters: [],
      returnType: 'string',
      childFunctions: [],
      lastModified: Date.now(),
      codeHash: '',
    });

    it('should use workflow mode when clicking test for an expr function', () => {
      // Setup: User clicks on a test for an expr function (workflow)
      const currentState: SelectionState = {
        mode: 'function',
        functionName: 'SomeOtherFunction',
        testName: null,
      };

      const target: EnrichedTarget = {
        name: 'EchoWorkflow',
        kind: 'test',
        exists: true,
        workflowMemberships: [], // Not in any workflow - it IS the workflow
        availableTests: ['EchoWorkflowTest'],
        functionName: 'EchoWorkflow',
        testName: 'EchoWorkflowTest',
      };

      // Context includes the expr function
      const context: NavigationContext = {
        workflows: [],
        functions: [
          createMockFunction('EchoWorkflow', 'function', 'expr'),
          createMockFunction('SomeOtherFunction', 'llm_function', 'llm'),
        ],
        bamlFiles: [],
        tests: [{ name: 'EchoWorkflowTest', functionName: 'EchoWorkflow', filePath: 'test.baml', nodeType: 'function' }],
      };

      const result = ruleEngine.decide(target, currentState, context);

      expect(result.rule).toBe('test-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.workflowId).toBe('EchoWorkflow');
        expect(result.state.selectedNodeId).toBe('EchoWorkflow|root:0');
        expect(result.state.testName).toBe('EchoWorkflowTest');
      }
    });

    it('should use function mode when clicking test for an LLM function', () => {
      // Setup: User clicks on a test for an LLM function
      const currentState: SelectionState = {
        mode: 'empty',
      };

      const target: EnrichedTarget = {
        name: 'ExtractUser',
        kind: 'test',
        exists: true,
        workflowMemberships: [],
        availableTests: ['ExtractUserTest'],
        functionName: 'ExtractUser',
        testName: 'ExtractUserTest',
      };

      // Context includes the LLM function (NOT expr)
      const context: NavigationContext = {
        workflows: [],
        functions: [
          createMockFunction('ExtractUser', 'llm_function', 'llm'),
        ],
        bamlFiles: [],
        tests: [{ name: 'ExtractUserTest', functionName: 'ExtractUser', filePath: 'test.baml', nodeType: 'llm_function' }],
      };

      const result = ruleEngine.decide(target, currentState, context);

      expect(result.rule).toBe('test-click');
      expect(result.state.mode).toBe('function');
      if (result.state.mode === 'function') {
        expect(result.state.functionName).toBe('ExtractUser');
        expect(result.state.testName).toBe('ExtractUserTest');
      }
    });

    it('should use workflow mode when clicking test for function that is in a workflow', () => {
      // Setup: User clicks on a test for a function that's called by a workflow
      const currentState: SelectionState = {
        mode: 'empty',
      };

      const target: EnrichedTarget = {
        name: 'HelperFunction',
        kind: 'test',
        exists: true,
        workflowMemberships: [{
          workflowId: 'MainWorkflow',
          nodeId: 'MainWorkflow|step1:0',
          nodeLabel: 'HelperFunction',
          calledFunctions: ['HelperFunction'],
        }],
        availableTests: ['HelperFunctionTest'],
        functionName: 'HelperFunction',
        testName: 'HelperFunctionTest',
      };

      const context: NavigationContext = {
        workflows: [],
        functions: [],
        bamlFiles: [],
        tests: [],
      };

      const result = ruleEngine.decide(target, currentState, context);

      expect(result.rule).toBe('test-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        expect(result.state.workflowId).toBe('MainWorkflow');
        expect(result.state.selectedNodeId).toBe('MainWorkflow|step1:0');
        expect(result.state.testName).toBe('HelperFunctionTest');
      }
    });

    it('should prefer workflow from workflowMemberships over context lookup', () => {
      // Setup: Function is both an expr function AND is called by another workflow
      // The workflowMemberships should take priority
      const currentState: SelectionState = {
        mode: 'empty',
      };

      const target: EnrichedTarget = {
        name: 'SharedHelper',
        kind: 'test',
        exists: true,
        workflowMemberships: [{
          workflowId: 'ParentWorkflow',
          nodeId: 'ParentWorkflow|step1:0',
          nodeLabel: 'SharedHelper',
          calledFunctions: ['SharedHelper'],
        }],
        availableTests: ['SharedHelperTest'],
        functionName: 'SharedHelper',
        testName: 'SharedHelperTest',
      };

      // Context shows SharedHelper is also an expr function
      const context: NavigationContext = {
        workflows: [],
        functions: [
          createMockFunction('SharedHelper', 'workflow', 'expr'),
        ],
        bamlFiles: [],
        tests: [],
      };

      const result = ruleEngine.decide(target, currentState, context);

      expect(result.rule).toBe('test-click');
      expect(result.state.mode).toBe('workflow');
      if (result.state.mode === 'workflow') {
        // Should use the workflow from memberships, not the function itself
        expect(result.state.workflowId).toBe('ParentWorkflow');
      }
    });
  });
});
