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
        functionName: 'extractUser',
        testName: 'test1',
        activeWorkflowId: null,
        selectedNodeId: null,
      });

      // Check that SDK atom was updated
      const sdkFunctionName = store.get(selectedFunctionNameAtom);
      expect(sdkFunctionName).toBe('extractUser');
    });

    it('should sync testName to SDK selectedTestCaseNameAtom', () => {
      store.set(unifiedSelectionAtom, {
        functionName: 'extractUser',
        testName: 'test_extract_valid_user',
        activeWorkflowId: null,
        selectedNodeId: null,
      });

      // Check that SDK atom was updated
      const sdkTestName = store.get(selectedTestCaseNameAtom);
      expect(sdkTestName).toBe('test_extract_valid_user');
    });

    it('should sync when using updater function', () => {
      store.set(unifiedSelectionAtom, {
        functionName: 'funcA',
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      });

      store.set(unifiedSelectionAtom, (prev) => ({
        ...prev,
        functionName: 'funcB',
      }));

      const sdkFunctionName = store.get(selectedFunctionNameAtom);
      expect(sdkFunctionName).toBe('funcB');
    });
  });

  describe('Unified Selection Atom', () => {
    it('should initialize with null values', () => {
      const selection = store.get(unifiedSelectionAtom);
      expect(selection).toEqual({
        functionName: null,
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      });
    });

    it('should update all fields when switching to a workflow', () => {
      store.set(unifiedSelectionAtom, {
        functionName: 'simpleWorkflow',
        testName: null,
        activeWorkflowId: 'simpleWorkflow',
        selectedNodeId: null,
      });

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.functionName).toBe('simpleWorkflow');
      expect(selection.activeWorkflowId).toBe('simpleWorkflow');
      expect(selection.selectedNodeId).toBeNull();
    });

    it('should update selectedNodeId when selecting a node in a workflow', () => {
      // First set up a workflow
      store.set(unifiedSelectionAtom, {
        functionName: 'simpleWorkflow',
        testName: null,
        activeWorkflowId: 'simpleWorkflow',
        selectedNodeId: null,
      });

      // Then select a node
      store.set(unifiedSelectionAtom, (prev) => ({
        ...prev,
        selectedNodeId: 'processData',
        functionName: 'processData',
      }));

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.selectedNodeId).toBe('processData');
      expect(selection.functionName).toBe('processData');
      expect(selection.activeWorkflowId).toBe('simpleWorkflow');
    });

    it('should clear activeWorkflowId for standalone functions', () => {
      store.set(unifiedSelectionAtom, {
        functionName: 'extractUser',
        testName: 'test_extract_valid_user',
        activeWorkflowId: null,
        selectedNodeId: null,
      });

      const selection = store.get(unifiedSelectionAtom);
      expect(selection.functionName).toBe('extractUser');
      expect(selection.activeWorkflowId).toBeNull();
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
      // With preview tab active and no node selected
      store.set(activeTabAtom, 'preview');
      store.set(unifiedSelectionAtom, {
        functionName: null,
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
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
        functionName: 'processData',
        testName: null,
        activeWorkflowId: 'simpleWorkflow',
        selectedNodeId: 'processData',
      });

      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('detail-panel');
    });

    it('should show test-panel on preview tab with no node selected', () => {
      store.set(activeTabAtom, 'preview');
      store.set(unifiedSelectionAtom, {
        functionName: 'extractUser',
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      });

      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('test-panel');
    });

    it('should show test-panel on curl tab', () => {
      store.set(activeTabAtom, 'curl');
      store.set(unifiedSelectionAtom, {
        functionName: 'extractUser',
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      });

      const mode = store.get(bottomPanelModeAtom);
      expect(mode).toBe('test-panel');
    });
  });

  describe('Navigation Scenarios - End-to-End State Updates', () => {
    it('Scenario: Click on workflow test → switch to workflow graph', () => {
      // Simulate: User clicks test_simple_success which tests simpleWorkflow
      store.set(unifiedSelectionAtom, {
        functionName: 'simpleWorkflow',
        testName: null,
        activeWorkflowId: 'simpleWorkflow',
        selectedNodeId: null,
      });
      store.set(activeTabAtom, 'graph');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);

      expect(selection.activeWorkflowId).toBe('simpleWorkflow');
      expect(tab).toBe('graph');
    });

    it('Scenario: Click on function in workflow → select node and show detail panel', () => {
      // Setup: Already in a workflow
      store.set(unifiedSelectionAtom, {
        functionName: 'simpleWorkflow',
        testName: null,
        activeWorkflowId: 'simpleWorkflow',
        selectedNodeId: null,
      });
      store.set(activeTabAtom, 'graph');

      // Action: Click on processData function
      store.set(unifiedSelectionAtom, (prev) => ({
        ...prev,
        selectedNodeId: 'processData',
        functionName: 'processData',
      }));

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);
      const bottomPanelMode = store.get(bottomPanelModeAtom);

      expect(selection.selectedNodeId).toBe('processData');
      expect(selection.activeWorkflowId).toBe('simpleWorkflow');
      expect(tab).toBe('graph');
      expect(bottomPanelMode).toBe('detail-panel');
    });

    it('Scenario: Click on function in different workflow → switch and select', () => {
      // Setup: Currently in simpleWorkflow
      store.set(unifiedSelectionAtom, {
        functionName: 'simpleWorkflow',
        testName: null,
        activeWorkflowId: 'simpleWorkflow',
        selectedNodeId: null,
      });

      // Action: Click handleSuccess which is in conditionalWorkflow
      store.set(unifiedSelectionAtom, {
        functionName: 'handleSuccess',
        testName: null,
        activeWorkflowId: 'conditionalWorkflow',
        selectedNodeId: 'handleSuccess',
      });
      store.set(activeTabAtom, 'graph');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);

      expect(selection.activeWorkflowId).toBe('conditionalWorkflow');
      expect(selection.selectedNodeId).toBe('handleSuccess');
      expect(tab).toBe('graph');
    });

    it('Scenario: Click on standalone LLM function → show prompt preview', () => {
      // Action: Click extractUser (standalone LLM function)
      store.set(unifiedSelectionAtom, {
        functionName: 'extractUser',
        testName: 'test_extract_valid_user',
        activeWorkflowId: null,
        selectedNodeId: null,
      });
      store.set(activeTabAtom, 'preview');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);
      const bottomPanelMode = store.get(bottomPanelModeAtom);

      expect(selection.functionName).toBe('extractUser');
      expect(selection.activeWorkflowId).toBeNull();
      expect(tab).toBe('preview');
      expect(bottomPanelMode).toBe('test-panel');
    });

    it('Scenario: Switch from workflow to standalone function', () => {
      // Setup: In a workflow
      store.set(unifiedSelectionAtom, {
        functionName: 'processData',
        testName: null,
        activeWorkflowId: 'simpleWorkflow',
        selectedNodeId: 'processData',
      });
      store.set(activeTabAtom, 'graph');

      // Action: Switch to standalone function
      store.set(unifiedSelectionAtom, {
        functionName: 'extractUser',
        testName: null,
        activeWorkflowId: null,
        selectedNodeId: null,
      });
      store.set(activeTabAtom, 'preview');

      const selection = store.get(unifiedSelectionAtom);
      const tab = store.get(activeTabAtom);
      const bottomPanelMode = store.get(bottomPanelModeAtom);

      expect(selection.activeWorkflowId).toBeNull();
      expect(selection.selectedNodeId).toBeNull();
      expect(tab).toBe('preview');
      expect(bottomPanelMode).toBe('test-panel');
    });
  });
});
