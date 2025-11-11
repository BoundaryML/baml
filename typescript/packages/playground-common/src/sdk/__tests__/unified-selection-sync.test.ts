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
  it('writes through to SDK atoms', () => {
    const store = createStore();

    store.set(unifiedSelectionAtom, {
      functionName: 'foo',
      testName: 'bar',
      activeWorkflowId: 'wf',
      selectedNodeId: 'node',
    });

    expect(store.get(selectedFunctionNameAtom)).toBe('foo');
    expect(store.get(selectedTestCaseNameAtom)).toBe('bar');
    expect(store.get(activeWorkflowIdAtom)).toBe('wf');
    expect(store.get(selectedNodeIdAtom)).toBe('node');
  });

  it('merges partial updates when using an updater function', () => {
    const store = createStore();

    store.set(unifiedSelectionAtom, {
      functionName: 'foo',
      testName: null,
      activeWorkflowId: 'wf',
      selectedNodeId: 'foo',
    });

    store.set(unifiedSelectionAtom, (prev) => ({
      ...prev,
      testName: 'tc',
    }));

    expect(store.get(selectedFunctionNameAtom)).toBe('foo');
    expect(store.get(selectedTestCaseNameAtom)).toBe('tc');
    expect(store.get(activeWorkflowIdAtom)).toBe('wf');
    expect(store.get(selectedNodeIdAtom)).toBe('foo');
  });

  it('reflects external atom changes when read', () => {
    const store = createStore();

    store.set(unifiedSelectionStateAtom, (prev) => ({
      ...prev,
      functionName: 'ExternalFunction',
    }));
    const selection = store.get(unifiedSelectionAtom);

    expect(selection.functionName).toBe('ExternalFunction');
    expect(selection.testName).toBeNull();
    expect(selection.activeWorkflowId).toBeNull();
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
  it('hides tabs for non-LLM workflow nodes', async () => {
    const store = await setupMockRuntimeStore();

    store.set(unifiedSelectionAtom, {
      functionName: 'fetchData',
      testName: null,
      activeWorkflowId: 'simpleWorkflow',
      selectedNodeId: 'fetchData',
    });

    const graphNodeView = store.get(viewModeAtom);
    expect(graphNodeView.showTabBar).toBe(true);
    expect(graphNodeView.showGraphTab).toBe(true);

    store.set(unifiedSelectionAtom, {
      functionName: 'processData',
      testName: null,
      activeWorkflowId: 'simpleWorkflow',
      selectedNodeId: 'processData',
    });

    const llmNodeView = store.get(viewModeAtom);
    expect(llmNodeView.showTabBar).toBe(true);
    expect(llmNodeView.showGraphTab).toBe(true);
  });
});
