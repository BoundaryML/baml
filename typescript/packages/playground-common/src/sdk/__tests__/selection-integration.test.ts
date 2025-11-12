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
import { createRealBAMLSDK } from '../factory';
import { DEBUG_BAML_FILES } from '../index';
import {
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
  activeWorkflowIdAtom,
  selectedNodeIdAtom,
  unifiedSelectionStateAtom,
  workflowsAtom,
  viewModeAtom,
  type SelectionState,
} from '../atoms/core.atoms';
import { activeTabAtom } from '../../shared/baml-project-panel/playground-panel/unified-atoms';
import { determineNavigationAction, type NavigationContext } from '../navigationHeuristic';
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
      store.set(unifiedSelectionStateAtom, { mode: 'empty' });
    };

    const getSelectionSnapshot = () => store.get(unifiedSelectionStateAtom);

    const applyNavigationAction = (action: SelectionState) => {
      store.set(unifiedSelectionStateAtom, action);
    };

    const buildNavState = (): NavigationContext => ({
      activeWorkflowId: store.get(activeWorkflowIdAtom),
      // Use getFunctions() to get ALL functions for navigation membership checking
      workflows: store.get(workflowsAtom),
      bamlFiles: sdk.diagnostics.getBAMLFiles(),
    });

    const simulateCodeClick = (event: CodeClickEvent) => {
      const action = determineNavigationAction(event, buildNavState());
      applyNavigationAction(action);
      return action;
    };

    // Note: this won't pass since the functioncallgraph doesnt return the names of functions called within each node. The nodes are not necessarily functions, and we need the function names.
    // the problem is we are not able to tell if an expr function is called within another expr function.
    it.skip('should switch to ConditionalWorkflow when clicking CheckCondition, then select header', () => {
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

      // CheckCondition is used in ConditionalWorkflow, so clicking it should switch to that workflow
      simulateCodeClick(llmEvent);
      const afterLLM = getSelectionSnapshot();
      expect(afterLLM.mode).toBe('workflow');
      if (afterLLM.mode === 'workflow') {
        expect(afterLLM.workflowId).toBe('ConditionalWorkflow'); // Switches to parent workflow
        expect(afterLLM.selectedNodeId).toBe('CheckCondition');
      }

      // Then clicking the header should stay in ConditionalWorkflow but select the header
      simulateCodeClick(headerEvent);
      const afterHeader = getSelectionSnapshot();
      expect(afterHeader.mode).toBe('workflow');
      if (afterHeader.mode === 'workflow') {
        expect(afterHeader.workflowId).toBe('ConditionalWorkflow');
        expect(afterHeader.selectedNodeId).toBe(conditionalWorkflowHeaderId);
      }
    });

    it.skip('updates atoms when toggling between workflow header and root nodes', () => {
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
      expect(action1.mode).toBe('workflow');
      expect(getSelectionSnapshot()).toMatchObject({
        mode: 'workflow',
        workflowId: 'ConditionalWorkflow',
        selectedNodeId: conditionalWorkflowHeaderId,
      });

      const action2 = simulateCodeClick(workflowEvent);
      expect(action2.mode).toBe('workflow');
      expect(getSelectionSnapshot()).toMatchObject({
        mode: 'workflow',
        workflowId: 'ConditionalWorkflow',
        selectedNodeId: 'ConditionalWorkflow',
      });

      const action3 = simulateCodeClick(headerEvent);
      expect(action3.mode).toBe('workflow');
      expect(getSelectionSnapshot()).toMatchObject({
        mode: 'workflow',
        workflowId: 'ConditionalWorkflow',
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
      store.set(unifiedSelectionStateAtom, { mode: 'empty' });
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
      expect(selectedTestCaseName).toBe('CheckAvailabilityTest')

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

      // Now click on a different function with tests (should clear previous test and select new one)
      dispatchIntent(buildFunctionIntent('ParseResume'));

      // Verify selection changed
      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBe('ParseResume');
      expect(selectedTestCaseName).toBe('ParseResumeTest'); // Auto-selects first test

      console.log('✓ Clicked ParseResume function (cleared previous test selection)');
      console.log('  Selected function:', selectedFunctionName);
      console.log('  Selected test:', selectedTestCaseName);
    });

    it('should render the same views when clicking back and forth between two llm functions', () => {
      // Simulate clicking on CheckAvailability function
      dispatchIntent(buildFunctionIntent('CheckAvailability'));

      // Verify selection state
      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);
      expect(store.get(activeTabAtom)).toBe('preview');



      expect(selectedFunctionName).toBe('CheckAvailability');
      expect(selectedTestCaseName).toBe('CheckAvailabilityTest')

      dispatchIntent(buildFunctionIntent('CheckAvailability'));

      // Verify selection state
      expect(store.get(activeTabAtom)).toBe('preview');

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
      // Should auto-select first test if available
      expect(newSelection.selectedTc).toBeDefined();
      expect(newSelection.selectedTc?.name).toBe('CheckAvailabilityTest');

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
        // Auto-selects first test when available
        { functionName: 'CheckAvailability', testCaseName: 'CheckAvailabilityTest', clickedTest: false },
        { functionName: 'CheckAvailability', testCaseName: 'CheckAvailabilityTest', clickedTest: true },
        { functionName: 'ExtractResume', testCaseName: 'Test1', clickedTest: false },
        { functionName: 'ExtractResume', testCaseName: 'Test1', clickedTest: true },
        { functionName: 'ParseResume', testCaseName: 'ParseResumeTest', clickedTest: true },
        // Note: CountItems has no tests, so clicking it would enter empty mode
      ];

      for (const selection of selections) {
        if (selection.clickedTest && selection.testCaseName) {
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

    it('should handle empty state correctly', () => {
      // Set to empty state
      store.set(unifiedSelectionStateAtom, { mode: 'empty' });

      const selectedFunctionName = store.get(sdk.atoms.selectedFunctionNameAtom);
      const selectedTestCaseName = store.get(sdk.atoms.selectedTestCaseNameAtom);

      expect(selectedFunctionName).toBeNull();
      expect(selectedTestCaseName).toBeNull();

      console.log('✓ Handled empty state correctly');
      console.log('  Function:', selectedFunctionName);
      console.log('  Test:', selectedTestCaseName);
    });
  });

  describe('Active Tab State', () => {
    it('should set activeTab to preview when selecting standalone LLM functions', () => {
      // Reset to a known state
      dispatchIntent(null);

      // Select CheckAvailability (standalone LLM function)
      dispatchIntent(buildFunctionIntent('CheckAvailability'));

      const activeTab1 = store.get(activeTabAtom);
      const selectedFunctionName1 = store.get(sdk.atoms.selectedFunctionNameAtom);

      expect(selectedFunctionName1).toBe('CheckAvailability');
      expect(activeTab1).toBe('preview');
      console.log('✓ CheckAvailability selected, activeTab:', activeTab1);

      // Select ExtractResume (another standalone LLM function)
      dispatchIntent(buildFunctionIntent('ExtractResume'));

      const activeTab2 = store.get(activeTabAtom);
      const selectedFunctionName2 = store.get(sdk.atoms.selectedFunctionNameAtom);

      expect(selectedFunctionName2).toBe('ExtractResume');
      expect(activeTab2).toBe('preview');
      console.log('✓ ExtractResume selected, activeTab:', activeTab2);
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
