import { describe, it, expect } from 'vitest';
import { createStore } from 'jotai';

import {
  selectedFunctionNameAtom,
  selectedTestCaseNameAtom,
  activeWorkflowIdAtom,
  selectedNodeIdAtom,
  runtimeInstanceAtom,
  unifiedSelectionStateAtom,
} from '../atoms/core.atoms';
import {
  unifiedSelectionAtom,
  viewModeAtom,
} from '../../shared/baml-project-panel/playground-panel/unified-atoms';
import { createMockSDK } from '../factory';

describe('unifiedSelectionAtom', () => {
  it('writes through to SDK atoms for function selection', () => {
    const store = createStore();

    store.set(unifiedSelectionAtom, {
      mode: 'function',
      functionName: 'foo',
      testName: 'bar',
    });

    expect(store.get(selectedFunctionNameAtom)).toBe('foo');
    expect(store.get(selectedTestCaseNameAtom)).toBe('bar');
  });

  it('writes through to SDK atoms for workflow selection', () => {
    const store = createStore();

    store.set(unifiedSelectionAtom, {
      mode: 'workflow',
      workflowId: 'wf',
      selectedNodeId: 'node',
      functionName: null,
      testName: null,
    });

    expect(store.get(activeWorkflowIdAtom)).toBe('wf');
    expect(store.get(selectedNodeIdAtom)).toBe('node');
  });

  it('merges partial updates when using an updater function', () => {
    const store = createStore();

    store.set(unifiedSelectionAtom, {
      mode: 'function',
      functionName: 'foo',
      testName: null,
    });

    store.set(unifiedSelectionAtom, (prev) => ({
      ...prev,
      testName: 'tc',
    }));

    const selection = store.get(unifiedSelectionAtom);
    expect(selection.mode).toBe('function');
    if (selection.mode === 'function') {
      expect(selection.functionName).toBe('foo');
      expect(selection.testName).toBe('tc');
    }
  });

  it('reflects external atom changes when read', () => {
    const store = createStore();

    store.set(unifiedSelectionStateAtom, {
      mode: 'function',
      functionName: 'ExternalFunction',
      testName: null,
    });
    const selection = store.get(unifiedSelectionAtom);

    expect(selection.mode).toBe('function');
    if (selection.mode === 'function') {
      expect(selection.functionName).toBe('ExternalFunction');
      expect(selection.testName).toBeNull();
    }
  });
});

async function setupMockRuntimeStore() {
  const store = createStore();
  const sdk = createMockSDK(store);
  await sdk.initialize({
    'workflows/simple.baml': '// mock workflow file',
  });
  return store;
}

describe('viewModeAtom', () => {
  // TODO: Fix this test - mock SDK workflow setup doesn't properly wire atoms
  // The test expects activeWorkflowAtom to have workflow data after setting
  // unifiedSelectionAtom, but the mock runtime's workflows aren't being
  // properly connected through the atom chain.
  it.skip('hides tabs for non-LLM workflow nodes', async () => {
    const store = await setupMockRuntimeStore();

    store.set(unifiedSelectionAtom, {
      mode: 'workflow',
      workflowId: 'simpleWorkflow',
      selectedNodeId: 'fetchData',
      functionName: null,
      testName: null,
    });

    const graphNodeView = store.get(viewModeAtom);
    expect(graphNodeView.showTabBar).toBe(true);
    expect(graphNodeView.showGraphTab).toBe(true);

    store.set(unifiedSelectionAtom, {
      mode: 'workflow',
      workflowId: 'simpleWorkflow',
      selectedNodeId: 'processData',
      functionName: null,
      testName: null,
    });

    const llmNodeView = store.get(viewModeAtom);
    expect(llmNodeView.showTabBar).toBe(true);
    expect(llmNodeView.showGraphTab).toBe(true);
  });
});
