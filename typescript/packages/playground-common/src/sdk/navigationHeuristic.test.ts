/**
 * Unit tests for navigation heuristic
 *
 * Tests all scenarios described in navigationHeuristic.ts documentation
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { determineNavigationAction, type NavigationContext } from './navigationHeuristic';
import type { CodeClickEvent, BAMLFile } from './types';
import type { FunctionWithCallGraph } from './interface';
import type { SelectionState } from './atoms/core.atoms';
import { createMockRuntimeConfig } from './mock-config/config';

// ============================================================================
// Test Data Setup - Using Centralized Mock Config
// ============================================================================

let mockWorkflows: FunctionWithCallGraph[];
let mockBAMLFiles: BAMLFile[];

beforeAll(() => {
  const mockConfig = createMockRuntimeConfig();
  mockWorkflows = mockConfig.workflows;
  mockBAMLFiles = mockConfig.bamlFiles;
});

// Helper to check if a selection is empty
const isEmptySelection = (selection: SelectionState): boolean => {
  return selection.mode === 'empty';
};

// ============================================================================
// Test Suites
// ============================================================================

describe('Navigation Heuristic - Test Click Events', () => {
  it('should switch to workflow when test targets a workflow', () => {
    const event: CodeClickEvent = {
      type: 'test',
      testName: 'test_simple_success',
      functionName: 'simpleWorkflow',
      filePath: 'workflows/simple.baml',
      nodeType: 'function',
    };

    const state: NavigationContext = {
      activeWorkflowId: null,
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    expect(action).toEqual({
      mode: 'workflow',
      workflowId: 'simpleWorkflow',
      selectedNodeId: 'simpleWorkflow',
      testName: 'test_simple_success',
    });
  });

  it('should switch to workflow and select node when test targets a function in a workflow', () => {
    const event: CodeClickEvent = {
      type: 'test',
      testName: 'test_fetchData_success',
      functionName: 'fetchData',
      filePath: 'workflows/simple.baml',
      nodeType: 'function',
    };

    const state: NavigationContext = {
      activeWorkflowId: null,
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    expect(action).toEqual({
      mode: 'workflow',
      workflowId: 'simpleWorkflow',
      selectedNodeId: 'fetchData',
      testName: 'test_fetchData_success',
    });
  });

  it('should switch to workflow and select node when test targets an LLM function in a workflow', () => {
    const event: CodeClickEvent = {
      type: 'test',
      testName: 'test_processData_valid',
      functionName: 'processData',
      filePath: 'workflows/simple.baml',
      nodeType: 'llm_function',
    };

    const state: NavigationContext = {
      activeWorkflowId: 'conditionalWorkflow', // Different workflow is active
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    expect(action).toEqual({
      mode: 'workflow',
      workflowId: 'simpleWorkflow',
      selectedNodeId: 'processData',
      testName: 'test_processData_valid',
    });
  });

  it('should show function tests when test targets a standalone function with tests', () => {
    const event: CodeClickEvent = {
      type: 'test',
      testName: 'test_extract_valid_user',
      functionName: 'extractUser',
      filePath: 'functions/utils.baml',
      nodeType: 'llm_function',
    };

    const state: NavigationContext = {
      activeWorkflowId: null,
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    expect(action).toEqual({
      mode: 'function',
      functionName: 'extractUser',
      testName: 'test_extract_valid_user',
    });
  });

  it('should show empty state when test targets a function with no workflow or tests', () => {
    const event: CodeClickEvent = {
      type: 'test',
      testName: 'test_unknown_function',
      functionName: 'unknownFunction',
      filePath: 'unknown.baml',
      nodeType: 'function',
    };

    const state: NavigationContext = {
      activeWorkflowId: null,
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    expect(action).toEqual({
      mode: 'empty',
    });
  });

  it('should stay in current workflow when test targets a function that exists in both current and other workflows', () => {
    // Setup: fetchData exists in both simpleWorkflow and sharedWorkflow (from mock config)
    const event: CodeClickEvent = {
      type: 'test',
      testName: 'test_fetchData_in_shared',
      functionName: 'fetchData',
      filePath: '/mock/sharedWorkflow.baml',
      nodeType: 'function',
    };

    const state: NavigationContext = {
      activeWorkflowId: 'sharedWorkflow', // Currently viewing sharedWorkflow
      workflows: mockWorkflows, // fetchData exists in simpleWorkflow and sharedWorkflow
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    // Should select the node in the current workflow, not switch to simpleWorkflow
    expect(action).toEqual({
      mode: 'workflow',
      workflowId: 'sharedWorkflow',
      selectedNodeId: 'fetchData',
      testName: 'test_fetchData_in_shared',
    });
  });
});

describe('Navigation Heuristic - Function Click Events', () => {
  describe('Priority 1: Stay in current workflow', () => {
    it('should select node when function exists in current workflow', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'processData',
        functionType: 'llm_function',
        filePath: 'workflows/simple.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: 'simpleWorkflow',
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'processData',
        testName: null,
      });
    });

    it('should select workflow node itself when clicking on workflow function in current workflow', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'simpleWorkflow',
        functionType: 'workflow',
        filePath: 'workflows/simple.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: 'simpleWorkflow',
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'simpleWorkflow',
        testName: null,
      });
    });
  });

  describe('Priority 2: Switch to workflow containing function', () => {
    it('should switch workflow when function exists in different workflow', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'handleSuccess',
        functionType: 'llm_function',
        filePath: 'workflows/conditional.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: 'simpleWorkflow',
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'workflow',
        workflowId: 'conditionalWorkflow',
        selectedNodeId: 'handleSuccess',
        testName: null,
      });
    });

    it('should switch to workflow when no current workflow is active', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'fetchData',
        functionType: 'function',
        filePath: 'workflows/simple.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: null,
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'fetchData',
        testName: null,
      });
    });

    it('should switch to workflow when function is a workflow itself', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'conditionalWorkflow',
        functionType: 'workflow',
        filePath: 'workflows/conditional.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: 'simpleWorkflow',
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'workflow',
        workflowId: 'conditionalWorkflow',
        selectedNodeId: 'conditionalWorkflow',
        testName: null,
      });
    });
  });

  describe('Priority 3: Show function in isolation (with or without tests)', () => {
    it('should show function with tests and auto-select first test', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'extractUser',
        functionType: 'llm_function',
        filePath: 'functions/utils.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: null,
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'function',
        functionName: 'extractUser',
        testName: 'test_extract_valid_user', // Auto-selects first test
      });
    });

    it('should show function even when it has no tests', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'helperFunction',
        functionType: 'function',
        filePath: 'functions/utils.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: null,
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'function',
        functionName: 'helperFunction',
        testName: null, // No tests, but function still shown
      });
    });
  });

  describe('Priority 4: Empty state', () => {

    it('should show empty state when function does not exist anywhere', () => {
      const event: CodeClickEvent = {
        type: 'function',
        functionName: 'nonExistentFunction',
        functionType: 'function',
        filePath: 'unknown.baml',
      };

      const state: NavigationContext = {
        activeWorkflowId: null,
        workflows: mockWorkflows,
        bamlFiles: mockBAMLFiles,
      };

      const action = determineNavigationAction(event, state);

      expect(action).toEqual({
        mode: 'empty',
      });
    });
  });
});

describe('Navigation Heuristic - Edge Cases', () => {
  it('should handle empty workflows list', () => {
    const event: CodeClickEvent = {
      type: 'function',
      functionName: 'someFunction',
      functionType: 'function',
      filePath: 'test.baml',
    };

    const state: NavigationContext = {
      activeWorkflowId: null,
      workflows: [],
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    // Function doesn't exist in BAML files, so should show empty state
    expect(action.mode).toBe('empty');
  });

  it('should handle empty BAML files list', () => {
    const event: CodeClickEvent = {
      type: 'function',
      functionName: 'someFunction',
      functionType: 'function',
      filePath: 'test.baml',
    };

    const state: NavigationContext = {
      activeWorkflowId: null,
      workflows: mockWorkflows,
      bamlFiles: [],
    };

    const action = determineNavigationAction(event, state);

    expect(action.mode).toBe('empty');
  });

  it('should handle workflow that does not exist in workflows list', () => {
    const event: CodeClickEvent = {
      type: 'function',
      functionName: 'someFunction',
      functionType: 'function',
      filePath: 'test.baml',
    };

    const state: NavigationContext = {
      activeWorkflowId: 'nonExistentWorkflow',
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    // Should skip Priority 1 (current workflow check) and continue to other priorities
    expect(action.mode).toBe('empty');
  });
});

describe('Navigation Heuristic - Complex Scenarios', () => {
  it('should prioritize current workflow over switching to different workflow', () => {
    // Function exists in both current workflow and another workflow
    // Should select node in current workflow (Priority 1)
    const event: CodeClickEvent = {
      type: 'function',
      functionName: 'fetchData',
      functionType: 'function',
      filePath: 'workflows/simple.baml',
    };

    const state: NavigationContext = {
      activeWorkflowId: 'simpleWorkflow',
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    expect(action.mode).toBe('workflow');
    if (action.mode === 'workflow') {
      expect(action.workflowId).toBe('simpleWorkflow');
    }
  });

  it('should handle function with multiple tests', () => {
    const event: CodeClickEvent = {
      type: 'function',
      functionName: 'simpleWorkflow',
      functionType: 'workflow',
      filePath: 'workflows/simple.baml',
    };

    const state: NavigationContext = {
      activeWorkflowId: 'simpleWorkflow',
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    const action = determineNavigationAction(event, state);

    // simpleWorkflow is in the current workflow, so should select it
    expect(action).toEqual({
      mode: 'workflow',
      workflowId: 'simpleWorkflow',
      selectedNodeId: 'simpleWorkflow',
      testName: null,
    });
  });

  it('should repeatedly select nodes when toggling between workflow and child nodes', () => {
    const initialState: NavigationContext = {
      activeWorkflowId: null,
      workflows: mockWorkflows,
      bamlFiles: mockBAMLFiles,
    };

    // First click on a function in a workflow - should switch to that workflow
    const switchAction = determineNavigationAction(
      {
        type: 'function',
        functionName: 'checkCondition',
        functionType: 'conditional',
        filePath: 'workflows/conditional.baml',
      },
      initialState
    );

    expect(switchAction).toEqual({
      mode: 'workflow',
      workflowId: 'conditionalWorkflow',
      selectedNodeId: 'checkCondition',
      testName: null,
    });

    // Now click on the workflow itself while already in that workflow
    const stateAfterSwitch: NavigationContext = {
      ...initialState,
      activeWorkflowId: 'conditionalWorkflow',
    };

    const workflowAction = determineNavigationAction(
      {
        type: 'function',
        functionName: 'conditionalWorkflow',
        functionType: 'workflow',
        filePath: 'workflows/conditional.baml',
      },
      stateAfterSwitch
    );

    expect(workflowAction).toEqual({
      mode: 'workflow',
      workflowId: 'conditionalWorkflow',
      selectedNodeId: 'conditionalWorkflow',
      testName: null,
    });

    // Click back to child node - should stay in workflow and select the child
    const backToChild = determineNavigationAction(
      {
        type: 'function',
        functionName: 'checkCondition',
        functionType: 'conditional',
        filePath: 'workflows/conditional.baml',
      },
      stateAfterSwitch
    );

    expect(backToChild).toEqual({
      mode: 'workflow',
      workflowId: 'conditionalWorkflow',
      selectedNodeId: 'checkCondition',
      testName: null,
    });
  });

});
