/**
 * Navigation Integration Tests
 *
 * Tests that verify the unified state atoms are correctly updated
 * when navigation actions are performed.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { createStore } from 'jotai';
import {
  unifiedSelectionAtom,
  activeTabAtom,
  viewModeAtom,
  bottomPanelModeAtom,
} from '../../shared/baml-project-panel/playground-panel/atoms';
import {
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
} from '../../sdk/atoms/core.atoms';

describe('Navigation Integration - Unified State', () => {
  let store: ReturnType<typeof createStore>;

  beforeEach(() => {
    store = createStore();
  });

  describe('SDK Atom Sync', () => {
    it('should sync functionName to SDK selectedFunctionNameAtom', () => {
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'extractUser',
        testName: 'test1',
      });

      // Check that SDK atom was updated
      const sdkFunctionName = store.get(selectedFunctionNameAtom);
      expect(sdkFunctionName).toBe('extractUser');
    });

    it('should sync testName to SDK selectedTestCaseNameAtom', () => {
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'extractUser',
        testName: 'test_extract_valid_user',
      });

      // Check that SDK atom was updated
      const sdkTestName = store.get(selectedTestCaseNameAtom);
      expect(sdkTestName).toBe('test_extract_valid_user');
    });

    it('should sync when using updater function', () => {
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'funcA',
        testName: null,
      });

      store.set(unifiedSelectionAtom, (prev) =>
        prev.mode === 'function'
          ? { ...prev, functionName: 'funcB' }
          : prev
      );

      const sdkFunctionName = store.get(selectedFunctionNameAtom);
      expect(sdkFunctionName).toBe('funcB');
    });
  });

  describe('Unified Selection Atom', () => {
    it('should initialize with empty mode', () => {
      const selection = store.get(unifiedSelectionAtom);
      expect(selection).toEqual({
        mode: 'empty',
      });
    });

    it('should update all fields when switching to a workflow', () => {
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'simpleWorkflow',
        functionName: null,
        testName: null,
      });

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.mode).toBe('workflow');
      if (selection.mode === 'workflow') {
        expect(selection.workflowId).toBe('simpleWorkflow');
        expect(selection.selectedNodeId).toBe('simpleWorkflow');
      }
    });

    it('should update selectedNodeId when selecting a node in a workflow', () => {
      // First set up a workflow
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'simpleWorkflow',
        functionName: null,
        testName: null,
      });

      // Then select a node
      store.set(unifiedSelectionAtom, (prev) =>
        prev.mode === 'workflow'
          ? { ...prev, selectedNodeId: 'processData' }
          : prev
      );

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.mode).toBe('workflow');
      if (selection.mode === 'workflow') {
        expect(selection.selectedNodeId).toBe('processData');
        expect(selection.workflowId).toBe('simpleWorkflow');
      }
    });

    it('should use function mode for standalone functions', () => {
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'extractUser',
        testName: 'test_extract_valid_user',
      });

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.mode).toBe('function');
      if (selection.mode === 'function') {
        expect(selection.functionName).toBe('extractUser');
      }
    });
  });

  describe('Active Tab Atom', () => {
    it('should initialize to preview', () => {
      const tab = store.get(activeTabAtom);
      expect(tab).toBe('preview');
    });

    it('should switch to graph when workflow is selected', () => {
      store.set(activeTabAtom, 'graph');
      expect(store.get(activeTabAtom)).toBe('graph');
    });

    it('should switch to preview for standalone functions', () => {
      store.set(activeTabAtom, 'graph');
      store.set(activeTabAtom, 'preview');
      expect(store.get(activeTabAtom)).toBe('preview');
    });

    it('should allow switching between all tab types', () => {
      store.set(activeTabAtom, 'preview');
      expect(store.get(activeTabAtom)).toBe('preview');

      store.set(activeTabAtom, 'curl');
      expect(store.get(activeTabAtom)).toBe('curl');

      store.set(activeTabAtom, 'graph');
      expect(store.get(activeTabAtom)).toBe('graph');
    });
  });

  describe('Bottom Panel Mode Atom', () => {
    it('should show test-panel by default', () => {
      // With preview tab active and empty selection
      store.set(activeTabAtom, 'preview');
      store.set(unifiedSelectionAtom, {
        mode: 'empty',
      });

      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('test-panel');
    });

    it('should show detail-panel when on graph tab', () => {
      store.set(activeTabAtom, 'graph');
      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('detail-panel');
    });

    it('should show detail-panel when node is selected (even on other tabs)', () => {
      store.set(activeTabAtom, 'preview');
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'processData',
        functionName: null,
        testName: null,
      });

      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('detail-panel');
    });

    it('should show detail-panel on preview tab with function selected', () => {
      store.set(activeTabAtom, 'preview');
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'extractUser',
        testName: null,
      });

      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('detail-panel');
    });

    it('should show detail-panel on curl tab with function selected', () => {
      store.set(activeTabAtom, 'curl');
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'extractUser',
        testName: null,
      });

      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('detail-panel');
    });
  });

  describe('Navigation Scenarios - End-to-End State Updates', () => {
    it('Scenario: Click on workflow test → switch to workflow graph', () => {
      // Simulate: User clicks test_simple_success which tests simpleWorkflow
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'simpleWorkflow',
        functionName: null,
        testName: null,
      });
      store.set(activeTabAtom, 'graph');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);

      expect(selection.mode).toBe('workflow');
      expect(tab).toBe('graph');
    });

    it('Scenario: Click on function in workflow → select node and show detail panel', () => {
      // Setup: Already in a workflow
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'simpleWorkflow',
        functionName: null,
        testName: null,
      });
      store.set(activeTabAtom, 'graph');

      // Action: Click on processData function
      store.set(unifiedSelectionAtom, (prev) =>
        prev.mode === 'workflow'
          ? { ...prev, selectedNodeId: 'processData' }
          : prev
      );

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);
      const bottomPanelMode = store.get(bottomPanelModeAtom);

      expect(selection.mode).toBe('workflow');
      if (selection.mode === 'workflow') {
        expect(selection.selectedNodeId).toBe('processData');
        expect(selection.workflowId).toBe('simpleWorkflow');
      }
      expect(tab).toBe('graph');
      expect(bottomPanelMode).toBe('detail-panel');
    });

    it('Scenario: Click on function in different workflow → switch and select', () => {
      // Setup: Currently in simpleWorkflow
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'simpleWorkflow',
        functionName: null,
        testName: null,
      });

      // Action: Click handleSuccess which is in conditionalWorkflow
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'conditionalWorkflow',
        selectedNodeId: 'handleSuccess',
        functionName: null,
        testName: null,
      });
      store.set(activeTabAtom, 'graph');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);

      expect(selection.mode).toBe('workflow');
      if (selection.mode === 'workflow') {
        expect(selection.workflowId).toBe('conditionalWorkflow');
        expect(selection.selectedNodeId).toBe('handleSuccess');
      }
      expect(tab).toBe('graph');
    });

    it('Scenario: Click on standalone LLM function → show prompt preview', () => {
      // Action: Click extractUser (standalone LLM function)
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'extractUser',
        testName: 'test_extract_valid_user',
      });
      store.set(activeTabAtom, 'preview');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);
      const bottomPanelMode = store.get(bottomPanelModeAtom);

      expect(selection.mode).toBe('function');
      if (selection.mode === 'function') {
        expect(selection.functionName).toBe('extractUser');
      }
      expect(tab).toBe('preview');
      expect(bottomPanelMode).toBe('detail-panel');
    });

    it('Scenario: Switch from workflow to standalone function', () => {
      // Setup: In a workflow
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'simpleWorkflow',
        selectedNodeId: 'processData',
        functionName: null,
        testName: null,
      });
      store.set(activeTabAtom, 'graph');

      // Action: Switch to standalone function
      store.set(unifiedSelectionAtom, {
        mode: 'function',
        functionName: 'extractUser',
        testName: null,
      });
      store.set(activeTabAtom, 'preview');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);
      const bottomPanelMode = store.get(bottomPanelModeAtom);

      expect(selection.mode).toBe('function');
      expect(tab).toBe('preview');
      expect(bottomPanelMode).toBe('detail-panel');
    });

    it('Scenario: Clicking workflow node twice should preserve testName', () => {
      // Setup: User is in a workflow with a test selected
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'test_simple_success',
      });

      const initialSelection = store.get(unifiedSelectionAtom);
      expect(initialSelection.mode).toBe('workflow');
      if (initialSelection.mode === 'workflow') {
        expect(initialSelection.testName).toBe('test_simple_success');
      }

      // Action: Click on the same root node again (simulating double-click)
      // The navigation system should preserve the testName
      store.set(unifiedSelectionAtom, (prev) => {
        if (prev.mode !== 'workflow') return prev;
        // This simulates what the navigation coordinator should do:
        // preserve testName when clicking the same node
        return {
          ...prev,
          selectedNodeId: 'SimpleWorkflow|root:0',
          // testName should be preserved, not reset to null
        };
      });

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.mode).toBe('workflow');
      if (selection.mode === 'workflow') {
        // The testName should still be preserved
        expect(selection.testName).toBe('test_simple_success');
        expect(selection.selectedNodeId).toBe('SimpleWorkflow|root:0');
        expect(selection.workflowId).toBe('SimpleWorkflow');
      }
    });

    it('Scenario: Clicking different workflow node should preserve testName', () => {
      // Setup: User is in a workflow with a test selected
      store.set(unifiedSelectionAtom, {
        mode: 'workflow',
        workflowId: 'SimpleWorkflow',
        selectedNodeId: 'SimpleWorkflow|root:0',
        functionName: 'SimpleWorkflow',
        testName: 'test_simple_success',
      });

      // Action: Click on a different node in the same workflow
      store.set(unifiedSelectionAtom, (prev) => {
        if (prev.mode !== 'workflow') return prev;
        return {
          ...prev,
          selectedNodeId: 'SimpleWorkflow|step1:0',
          functionName: 'ProcessStep',
          // testName should be preserved when clicking different nodes
        };
      });

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.mode).toBe('workflow');
      if (selection.mode === 'workflow') {
        expect(selection.testName).toBe('test_simple_success');
        expect(selection.selectedNodeId).toBe('SimpleWorkflow|step1:0');
      }
    });
  });
});
