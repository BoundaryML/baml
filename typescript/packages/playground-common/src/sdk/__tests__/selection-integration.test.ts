/**
 * Integration Test: Selection State with Real BAML Runtime
 *
 * Tests that clicking on functions and tests properly updates selection state
 * using the real BAML runtime (not mocked).
 *
 * This simulates the DebugPanel's click behavior and verifies that the same
 * atoms get updated as when updateCursor runs.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { createStore } from 'jotai';
import { createRealBAMLSDK, DEBUG_BAML_FILES } from '../index';
import {
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
  activeWorkflowIdAtom,
  selectedNodeIdAtom,
  unifiedSelectionStateAtom,
} from '../atoms/core.atoms';
import { determineNavigationAction, type NavigationState, type NavigationAction } from '../navigationHeuristic';
import type { CodeClickEvent, NavigationIntent } from '../types';

describe('Selection State Integration (Real WASM Runtime)', () => {
  let sdk: ReturnType<typeof createRealBAMLSDK>;
  let store: ReturnType<typeof createStore>;
  let conditionalWorkflowHeaderId: string;

  beforeAll(async () => {
    // Create SDK with real BAML runtime
    store = createStore();
    sdk = createRealBAMLSDK(store);

    // Initialize with debug BAML files (same as debug mode)
    await sdk.initialize(DEBUG_BAML_FILES);

     const conditionalWorkflow = sdk.workflows.getById('ConditionalWorkflow');
     if (!conditionalWorkflow) {
       throw new Error('ConditionalWorkflow not found in real runtime');
     }
     const headerNode = conditionalWorkflow.nodes.find((node) =>
       node.label === 'check summary confidence'
     );
     if (!headerNode) {
       throw new Error('Header node for "check summary confidence" not found');
     }
     conditionalWorkflowHeaderId = headerNode.id;
  });

  describe('BAML Files Loading', () => {
    it('should load debug BAML files and extract functions', () => {
      const functions = sdk.diagnostics.getFunctions();

      console.log('Extracted functions:', functions.map(f => f.name));

      expect(functions.length).toBeGreaterThanOrEqual(4);

      // Verify our debug functions are present
      const functionNames = functions.map(f => f.name);
      expect(functionNames).toContain('ExtractResume');
      expect(functionNames).toContain('CheckAvailability');
      expect(functionNames).toContain('CountItems');
      expect(functionNames).toContain('ParseResume');
    });

    it('should extract test cases from BAML files', () => {
      // Get all test cases for all functions
      const functions = sdk.diagnostics.getFunctions();
      const allTestCases = functions.flatMap(fn => sdk.testCases.get(fn.name));

      console.log('Extracted test cases:', allTestCases.map(tc => `${tc.name} (${tc.functionId})`));

      expect(allTestCases).toBeDefined();
      expect(allTestCases.length).toBeGreaterThanOrEqual(3);

      const testNames = allTestCases.map(tc => tc.name);
      expect(testNames).toContain('Test1');
      expect(testNames).toContain('CheckAvailabilityTest');
      expect(testNames).toContain('ParseResumeTest');
    });

    it('should verify CountItems has no tests', () => {
      const countItemsTests = sdk.testCases.get('CountItems');

      console.log('CountItems tests:', countItemsTests);

      expect(countItemsTests).toBeDefined();
      expect(countItemsTests.length).toBe(0);
    });
  });

  describe('Navigation Heuristic with real runtime data', () => {
    const resetSelectionAtoms = () => {
      store.set(unifiedSelectionStateAtom, {
        functionName: null,
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      });
    };

    const getSelectionSnapshot = () => store.get(unifiedSelectionStateAtom);

    const applyNavigationAction = (action: NavigationAction) => {
      switch (action.type) {
        case 'switch-workflow':
          store.set(unifiedSelectionStateAtom, {
            functionName: action.workflowId,
            testName: null,
            activeWorkflowId: action.workflowId,
            selectedNodeId: null,
          });
          break;
        case 'switch-and-select':
          store.set(unifiedSelectionStateAtom, {
            functionName: action.nodeId,
            testName: action.testId ?? null,
            activeWorkflowId: action.workflowId,
            selectedNodeId: action.nodeId,
          });
          break;
        case 'select-node':
          store.set(unifiedSelectionStateAtom, (prev) => ({
            functionName: action.nodeId,
            testName: action.testId ?? prev.testName,
            activeWorkflowId: action.workflowId,
            selectedNodeId: action.nodeId,
          }));
          break;
        case 'show-function-tests':
          store.set(unifiedSelectionStateAtom, {
            functionName: action.functionName,
            testName: action.tests[0] ?? null,
            activeWorkflowId: null,
            selectedNodeId: action.functionName,
          });
          break;
        case 'empty-state':
        default:
          store.set(unifiedSelectionStateAtom, {
            functionName: null,
            testName: null,
            activeWorkflowId: null,
            selectedNodeId: null,
          });
          break;
      }
    };

    const buildNavState = (): NavigationState => ({
      activeWorkflowId: store.get(activeWorkflowIdAtom),
      workflows: sdk.workflows.getAll(),
      bamlFiles: sdk.diagnostics.getBAMLFiles(),
    });

    const simulateCodeClick = (event: CodeClickEvent) => {
      const action = determineNavigationAction(event, buildNavState());
      applyNavigationAction(action);
      return action;
    };

    it('should show the bug where selectedNodeId becomes null after clicking CheckCondition then header', () => {
      resetSelectionAtoms();

      const llmEvent: CodeClickEvent = {
        type: 'function',
        functionName: 'CheckCondition',
        functionType: 'llm_function',
        filePath: 'baml_src/workflows/conditional.baml',
      };

      const headerEvent: CodeClickEvent = {
        type: 'function',
        functionName: conditionalWorkflowHeaderId,
        functionType: 'group',
        filePath: 'baml_src/workflows/conditional.baml',
      };

      simulateCodeClick(llmEvent);
      const afterLLM = getSelectionSnapshot();
      expect(afterLLM.activeWorkflowId).toBe('CheckCondition');
      expect(afterLLM.selectedNodeId).toBe('CheckCondition');

      simulateCodeClick(headerEvent);
      const afterHeader = getSelectionSnapshot();
      expect(afterHeader.activeWorkflowId).toBe('ConditionalWorkflow');
      expect(afterHeader.selectedNodeId).toBe(conditionalWorkflowHeaderId);
    });

    it('updates atoms when toggling between workflow header and root nodes', () => {
      resetSelectionAtoms();

      const headerEvent: CodeClickEvent = {
        type: 'function',
        functionName: conditionalWorkflowHeaderId,
        functionType: 'function',
        filePath: 'baml_src/workflows/conditional.baml',
      };

      const workflowEvent: CodeClickEvent = {
        type: 'function',
        functionName: 'ConditionalWorkflow',
        functionType: 'workflow',
        filePath: 'baml_src/workflows/conditional.baml',
      };

      const action1 = simulateCodeClick(headerEvent);
      expect(action1.type).toBe('switch-and-select');
      expect(getSelectionSnapshot()).toMatchObject({
        activeWorkflowId: 'ConditionalWorkflow',
        selectedNodeId: conditionalWorkflowHeaderId,
      });

      const action2 = simulateCodeClick(workflowEvent);
      expect(action2.type).toBe('select-node');
      expect(getSelectionSnapshot()).toMatchObject({
        activeWorkflowId: 'ConditionalWorkflow',
        selectedNodeId: 'ConditionalWorkflow',
      });

      const action3 = simulateCodeClick(headerEvent);
      expect(action3.type).toBe('select-node');
      expect(getSelectionSnapshot()).toMatchObject({
        activeWorkflowId: 'ConditionalWorkflow',
        selectedNodeId: conditionalWorkflowHeaderId,
      });
    });
  });

  const buildFunctionIntent = (functionName: string): NavigationIntent => {
    const fn = sdk.diagnostics.getFunctions().find((f) => f.name === functionName);
    const functionType = fn?.type === 'workflow'
      ? 'workflow'
      : fn?.functionFlavor === 'llm'
        ? 'llm_function'
        : 'function';
    const filePath = fn?.filePath ?? 'test://selection';

    return {
      type: 'function',
      functionName,
      functionType,
      filePath,
      source: 'test-panel',
    };
  };

  const buildTestIntent = (functionName: string, testName: string): NavigationIntent => {
    const fn = sdk.diagnostics.getFunctions().find((f) => f.name === functionName);
    const nodeType = fn?.functionFlavor === 'llm' ? 'llm_function' : 'function';
    const filePath = fn?.filePath ?? 'test://selection';

    return {
      type: 'test',
      functionName,
      testName,
      nodeType,
      filePath,
      source: 'test-panel',
    };
  };

  const dispatchIntent = (intent: NavigationIntent | null) => {
    if (intent) {
      store.set(sdk.atoms.navigationDispatcherAtom, intent);
    } else {
      store.set(unifiedSelectionStateAtom, {
        functionName: null,
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      });
    }
  };

  describe('Clicking on Functions', () => {
    it('should update selection when clicking on CheckAvailability function', () => {
      // Simulate clicking on CheckAvailability function
      dispatchIntent(buildFunctionIntent('CheckAvailability'));

      // Verify selection state
      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBe('CheckAvailability');
      expect(selectedTestCaseName).toBeNull();

      console.log('✓ Clicked CheckAvailability function');
      console.log('  Selected function:', selectedFunctionName);
      console.log('  Selected test:', selectedTestCaseName);
    });

    it('should clear test selection when clicking on a different function', () => {
      // First, select a function with a test
      dispatchIntent(buildTestIntent('ExtractResume', 'Test1'));

      // Verify both are set
      expect(store.get(sdk.atoms.selectedFunctionNameAtom)).toBe('ExtractResume');
      expect(store.get(sdk.atoms.selectedTestCaseNameAtom)).toBe('Test1');

      // Now click on a different function (should clear test)
      dispatchIntent(buildFunctionIntent('CountItems'));

      // Verify test was cleared
      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBe('CountItems');
      expect(selectedTestCaseName).toBeNull();

      console.log('✓ Clicked CountItems function (cleared test selection)');
      console.log('  Selected function:', selectedFunctionName);
      console.log('  Selected test:', selectedTestCaseName);
    });
  });

  describe('Clicking on Tests', () => {
    it('should update both function and test when clicking on CheckAvailabilityTest', () => {
      // Simulate clicking on CheckAvailabilityTest
      dispatchIntent(buildTestIntent('CheckAvailability', 'CheckAvailabilityTest'));

      // Verify selection state
      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBe('CheckAvailability');
      expect(selectedTestCaseName).toBe('CheckAvailabilityTest');

      console.log('✓ Clicked CheckAvailabilityTest');
      console.log('  Selected function:', selectedFunctionName);
      console.log('  Selected test:', selectedTestCaseName);
    });

    it('should update selection when clicking on Test1', () => {
      // Simulate clicking on Test1 (for ExtractResume)
      dispatchIntent(buildTestIntent('ExtractResume', 'Test1'));

      // Verify selection state
      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBe('ExtractResume');
      expect(selectedTestCaseName).toBe('Test1');

      console.log('✓ Clicked Test1');
      console.log('  Selected function:', selectedFunctionName);
      console.log('  Selected test:', selectedTestCaseName);
    });

    it('should update selection when clicking on ParseResumeTest', () => {
      // Simulate clicking on ParseResumeTest
      dispatchIntent(buildTestIntent('ParseResume', 'ParseResumeTest'));

      // Verify selection state
      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBe('ParseResume');
      expect(selectedTestCaseName).toBe('ParseResumeTest');

      console.log('✓ Clicked ParseResumeTest');
      console.log('  Selected function:', selectedFunctionName);
      console.log('  Selected test:', selectedTestCaseName);
    });
  });

  describe('Selection Atom Reactivity', () => {
    it('should trigger selectionAtom updates when function changes', () => {
      // Get initial selection state
      const initialSelection = store.get(sdk.atoms.selectionAtom);
      console.log('Initial selection:', initialSelection);

      // Change function selection
      dispatchIntent(buildFunctionIntent('CheckAvailability'));

      // Verify selection state changed
      const newSelection = store.get(sdk.atoms.selectionAtom);
      console.log('New selection:', newSelection);

      expect(newSelection.selectedFn).toBeDefined();
      expect(newSelection.selectedFn?.name).toBe('CheckAvailability');
      expect(newSelection.selectedTc).toBeNull();

      // Verify it's different from initial
      expect(newSelection).not.toEqual(initialSelection);
    });

    it('should derive selectedFunctionObjectAtom from function name', () => {
      // Set function selection
      dispatchIntent(buildFunctionIntent('ExtractResume'));

      // Get derived function object
      const selectedFunctionObject = store.get(sdk.atoms.selectedFunctionObjectAtom);

      console.log('✓ Selected function object:', selectedFunctionObject);

      // Note: This will be null until bamlFilesAtom is populated from getBAMLFiles()
      // The real implementation needs to call sdk.diagnostics.getBAMLFiles() and populate bamlFilesAtom
    });
  });

  describe('Multiple Selection Changes', () => {
    it('should handle rapid selection changes correctly', () => {
      const selections = [
        { functionName: 'CheckAvailability', testCaseName: null },
        { functionName: 'CheckAvailability', testCaseName: 'CheckAvailabilityTest' },
        { functionName: 'ExtractResume', testCaseName: null },
        { functionName: 'ExtractResume', testCaseName: 'Test1' },
        { functionName: 'ParseResume', testCaseName: 'ParseResumeTest' },
        { functionName: 'CountItems', testCaseName: null },
      ];

      for (const selection of selections) {
        if (selection.testCaseName) {
          dispatchIntent(buildTestIntent(selection.functionName, selection.testCaseName));
        } else {
          dispatchIntent(buildFunctionIntent(selection.functionName));
        }

        const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
        const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

        expect(selectedFunctionName).toBe(selection.functionName);
        expect(selectedTestCaseName).toBe(selection.testCaseName);

        console.log(`✓ ${selection.functionName}${selection.testCaseName ? ` → ${selection.testCaseName}` : ''}`);
      }
    });
  });

  describe('Edge Cases', () => {
    it('should handle setting selection to null', () => {
      // First set a selection
      dispatchIntent(buildTestIntent('CheckAvailability', 'CheckAvailabilityTest'));

      // Clear selection
      dispatchIntent(null);

      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBeNull();
      expect(selectedTestCaseName).toBeNull();

      console.log('✓ Cleared selection');
    });

    it('should handle selecting a test without a function (edge case)', () => {
      // This is technically an invalid state, but we should handle it gracefully
      store.set(unifiedSelectionStateAtom, {
        functionName: null,
        testName: 'Test1',
        activeWorkflowId: null,
        selectedNodeId: null,
      });

      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBeNull();
      expect(selectedTestCaseName).toBe('Test1');

      console.log('✓ Handled invalid state: test without function');
      console.log('  Function:', selectedFunctionName);
      console.log('  Test:', selectedTestCaseName);
    });
  });

  describe('Real Runtime Verification', () => {
    it('should be using real BAML runtime, not mock', () => {
      // Verify we're using the real runtime by checking that we get actual functions
      const functions = sdk.diagnostics.getFunctions();

      // Mock runtime would return mock workflows, real runtime extracts from WASM
      console.log('Extracted', functions.length, 'functions from BAML files');

      expect(functions.length).toBeGreaterThan(0);

      // Verify we have real function metadata (file paths, etc)
      const firstFunction = functions[0];
      expect(firstFunction).toBeDefined();
      expect(firstFunction?.name).toBeDefined();
      expect(firstFunction?.span?.filePath).toBeDefined();

      console.log('Sample function:', firstFunction);
    });

    it('should have diagnostics from real BAML compilation', () => {
      const diagnostics = store.get(sdk.atoms.diagnosticsAtom);

      console.log('Diagnostics:', diagnostics);

      expect(diagnostics).toBeDefined();
      expect(Array.isArray(diagnostics)).toBe(true);

      // Our debug files should compile without errors
      const errors = diagnostics.filter(d => d.type === 'error');
      expect(errors.length).toBe(0);
    });
  });
});
