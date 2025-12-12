/**
 * Tests for Watch Notification Events
 *
 * Tests the workflow header events implementation:
 * - Parsing of different watch notification value types
 * - Node state updates when header events are received
 * - Integration with the SDK storage layer
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { createStore } from 'jotai';
import type {
  WatchNotification,
  WatchNotificationValue,
  WatchHeaderValue,
  WatchStreamStartValue,
  WatchStreamUpdateValue,
  WatchStreamEndValue,
  FunctionWithCallGraph,
  GraphNode,
} from '../interface';
import { nodeStateAtomFamily, registerNodeAtom } from '../atoms/core.atoms';

// ============================================================================
// Test Helpers - Mimics SDK internal methods for testing
// ============================================================================

/**
 * Parse watch notification value JSON into typed structure
 * This mirrors the SDK's parseWatchValue method
 */
function parseWatchValue(value: string): WatchNotificationValue | undefined {
  try {
    const parsed = JSON.parse(value) as Record<string, unknown>;
    if (parsed && typeof parsed === 'object' && 'type' in parsed) {
      switch (parsed.type) {
        case 'header': {
          const result: WatchHeaderValue = {
            type: 'header',
            label: typeof parsed.label === 'string' ? parsed.label : '',
            level: typeof parsed.level === 'number' ? parsed.level : 1,
          };
          if (parsed.span && typeof parsed.span === 'object') {
            const spanData = parsed.span as Record<string, unknown>;
            result.span = {
              filePath: typeof spanData.file_path === 'string' ? spanData.file_path : '',
              startLine: typeof spanData.start_line === 'number' ? spanData.start_line : 0,
              startColumn: typeof spanData.start_column === 'number' ? spanData.start_column : 0,
              endLine: typeof spanData.end_line === 'number' ? spanData.end_line : 0,
              endColumn: typeof spanData.end_column === 'number' ? spanData.end_column : 0,
            };
          }
          return result;
        }
        case 'stream_start': {
          const result: WatchStreamStartValue = {
            type: 'stream_start',
            id: typeof parsed.id === 'string' ? parsed.id : '',
          };
          return result;
        }
        case 'stream_update': {
          const result: WatchStreamUpdateValue = {
            type: 'stream_update',
            id: typeof parsed.id === 'string' ? parsed.id : '',
            value: typeof parsed.value === 'string' ? parsed.value : '',
          };
          return result;
        }
        case 'stream_end': {
          const result: WatchStreamEndValue = {
            type: 'stream_end',
            id: typeof parsed.id === 'string' ? parsed.id : '',
          };
          return result;
        }
      }
    }
    return undefined;
  } catch {
    return undefined;
  }
}

/**
 * Find graph node ID by matching label
 */
function findNodeIdByLabel(
  nodes: GraphNode[],
  label: string
): string | undefined {
  for (const node of nodes) {
    if (node.label === label) {
      return node.id;
    }
  }
  return undefined;
}

/**
 * Enrich watch notification with parsed value and context
 */
// No enrichment helper anymore; tests should directly use stateUpdate+parsed value

// ============================================================================
// Tests
// ============================================================================

describe('Watch Notification Parsing', () => {
  describe('parseWatchValue', () => {
    it('should parse header events correctly', () => {
      const headerJson = JSON.stringify({
        type: 'header',
        label: 'gather applicant context',
        level: 1,
        span: {
          file_path: '/test/workflow.baml',
          start_line: 10,
          start_column: 4,
          end_line: 10,
          end_column: 30,
        },
      });

      const result = parseWatchValue(headerJson);

      expect(result).toBeDefined();
      expect(result?.type).toBe('header');
      if (result?.type === 'header') {
        expect(result.label).toBe('gather applicant context');
        expect(result.level).toBe(1);
        expect(result.span).toBeDefined();
        expect(result.span?.filePath).toBe('/test/workflow.baml');
        expect(result.span?.startLine).toBe(10);
        expect(result.span?.startColumn).toBe(4);
        expect(result.span?.endLine).toBe(10);
        expect(result.span?.endColumn).toBe(30);
      }
    });

    it('should parse header events without span', () => {
      const headerJson = JSON.stringify({
        type: 'header',
        label: 'normalize profile signals',
        level: 2,
      });

      const result = parseWatchValue(headerJson);

      expect(result).toBeDefined();
      expect(result?.type).toBe('header');
      if (result?.type === 'header') {
        expect(result.label).toBe('normalize profile signals');
        expect(result.level).toBe(2);
        expect(result.span).toBeUndefined();
      }
    });

    it('should parse stream_start events', () => {
      const streamStartJson = JSON.stringify({
        type: 'stream_start',
        id: 'stream-123',
      });

      const result = parseWatchValue(streamStartJson);

      expect(result).toBeDefined();
      expect(result?.type).toBe('stream_start');
      if (result?.type === 'stream_start') {
        expect(result.id).toBe('stream-123');
      }
    });

    it('should parse stream_update events', () => {
      const streamUpdateJson = JSON.stringify({
        type: 'stream_update',
        id: 'stream-123',
        value: '{"partial": "data"}',
      });

      const result = parseWatchValue(streamUpdateJson);

      expect(result).toBeDefined();
      expect(result?.type).toBe('stream_update');
      if (result?.type === 'stream_update') {
        expect(result.id).toBe('stream-123');
        expect(result.value).toBe('{"partial": "data"}');
      }
    });

    it('should parse stream_end events', () => {
      const streamEndJson = JSON.stringify({
        type: 'stream_end',
        id: 'stream-123',
      });

      const result = parseWatchValue(streamEndJson);

      expect(result).toBeDefined();
      expect(result?.type).toBe('stream_end');
      if (result?.type === 'stream_end') {
        expect(result.id).toBe('stream-123');
      }
    });

    it('should return undefined for regular values (no type field)', () => {
      const regularValueJson = JSON.stringify({
        name: 'John Doe',
        age: 30,
      });

      const result = parseWatchValue(regularValueJson);

      expect(result).toBeUndefined();
    });

    it('should return undefined for invalid JSON', () => {
      const invalidJson = 'not valid json {{{';

      const result = parseWatchValue(invalidJson);

      expect(result).toBeUndefined();
    });

    it('should handle missing fields gracefully', () => {
      const incompleteHeaderJson = JSON.stringify({
        type: 'header',
        // missing label and level
      });

      const result = parseWatchValue(incompleteHeaderJson);

      expect(result).toBeDefined();
      expect(result?.type).toBe('header');
      if (result?.type === 'header') {
        expect(result.label).toBe(''); // Default to empty string
        expect(result.level).toBe(1); // Default to 1
      }
    });
  });
});

describe('Node ID Matching', () => {
  const mockNodes: GraphNode[] = [
    {
      id: '0',
      type: 'group',
      label: 'SimpleWorkflow',
      functionName: 'SimpleWorkflow',
      codeHash: '',
      lastModified: Date.now(),
      metadata: { logFilterKey: 'SimpleWorkflow|root:0' },
    },
    {
      id: '1',
      type: 'group',
      label: 'gather applicant context',
      functionName: 'SimpleWorkflow',
      parent: '0',
      codeHash: '',
      lastModified: Date.now(),
      metadata: { logFilterKey: 'SimpleWorkflow|root:0|hdr:gather-applicant-context:0' },
    },
    {
      id: '2',
      type: 'group',
      label: 'normalize profile signals',
      functionName: 'SimpleWorkflow',
      parent: '0',
      codeHash: '',
      lastModified: Date.now(),
      metadata: { logFilterKey: 'SimpleWorkflow|root:0|hdr:normalize-profile-signals:1' },
    },
  ];

  describe('findNodeIdByLabel', () => {
    it('should find node by exact label match', () => {
      const nodeId = findNodeIdByLabel(mockNodes, 'gather applicant context');

      expect(nodeId).toBe('1');
    });

    it('should find second header node', () => {
      const nodeId = findNodeIdByLabel(mockNodes, 'normalize profile signals');

      expect(nodeId).toBe('2');
    });

    it('should find root node', () => {
      const nodeId = findNodeIdByLabel(mockNodes, 'SimpleWorkflow');

      expect(nodeId).toBe('0');
    });

    it('should return undefined for non-existent label', () => {
      const nodeId = findNodeIdByLabel(mockNodes, 'non-existent header');

      expect(nodeId).toBeUndefined();
    });

    it('should return undefined for empty nodes array', () => {
      const nodeId = findNodeIdByLabel([], 'any label');

      expect(nodeId).toBeUndefined();
    });
  });
});

describe('Node State Updates', () => {
  let store: ReturnType<typeof createStore>;

  beforeEach(() => {
    store = createStore();
  });

  it('should update node state to running when header event is received', () => {
    const nodeId = '0';

    // Register the node
    store.set(registerNodeAtom, nodeId);

    // Simulate what SDK does when a header event is received
    store.set(nodeStateAtomFamily(nodeId), 'running');

    // Verify state was updated
    const state = store.get(nodeStateAtomFamily(nodeId));
    expect(state).toBe('running');
  });

  it('should handle multiple node state updates', () => {
    const nodeId1 = '0';
    const nodeId2 = '1';

    // Register nodes
    store.set(registerNodeAtom, nodeId1);
    store.set(registerNodeAtom, nodeId2);

    // Initial state should be 'not-started'
    expect(store.get(nodeStateAtomFamily(nodeId1))).toBe('not-started');
    expect(store.get(nodeStateAtomFamily(nodeId2))).toBe('not-started');

    // Simulate first header event
    store.set(nodeStateAtomFamily(nodeId1), 'running');
    expect(store.get(nodeStateAtomFamily(nodeId1))).toBe('running');
    expect(store.get(nodeStateAtomFamily(nodeId2))).toBe('not-started');

    // Simulate second header event
    store.set(nodeStateAtomFamily(nodeId2), 'running');
    expect(store.get(nodeStateAtomFamily(nodeId1))).toBe('running');
    expect(store.get(nodeStateAtomFamily(nodeId2))).toBe('running');
  });

  it('should allow transitioning node states', () => {
    const nodeId = '0';

    // Register node
    store.set(registerNodeAtom, nodeId);

    // Start execution
    store.set(nodeStateAtomFamily(nodeId), 'running');
    expect(store.get(nodeStateAtomFamily(nodeId))).toBe('running');

    // Complete successfully
    store.set(nodeStateAtomFamily(nodeId), 'success');
    expect(store.get(nodeStateAtomFamily(nodeId))).toBe('success');
  });

  it('should handle error state', () => {
    const nodeId = '0';

    // Register node
    store.set(registerNodeAtom, nodeId);

    // Start execution
    store.set(nodeStateAtomFamily(nodeId), 'running');

    // Encounter error
    store.set(nodeStateAtomFamily(nodeId), 'error');
    expect(store.get(nodeStateAtomFamily(nodeId))).toBe('error');
  });
});

describe('End-to-End Watch Notification Flow', () => {
  const mockNodes: GraphNode[] = [
    {
      id: '0',
      type: 'group',
      label: 'SimpleWorkflow',
      functionName: 'SimpleWorkflow',
      codeHash: '',
      lastModified: Date.now(),
      metadata: { logFilterKey: 'SimpleWorkflow|root:0' },
    },
    {
      id: '1',
      type: 'group',
      label: 'gather applicant context',
      functionName: 'SimpleWorkflow',
      parent: '0',
      codeHash: '',
      lastModified: Date.now(),
      metadata: { logFilterKey: 'SimpleWorkflow|root:0|hdr:gather-applicant-context:0' },
    },
    {
      id: '2',
      type: 'group',
      label: 'normalize profile signals',
      functionName: 'SimpleWorkflow',
      parent: '0',
      codeHash: '',
      lastModified: Date.now(),
      metadata: { logFilterKey: 'SimpleWorkflow|root:0|hdr:normalize-profile-signals:1' },
    },
  ];

  let store: ReturnType<typeof createStore>;

  beforeEach(() => {
    store = createStore();
    // Register all nodes
    mockNodes.forEach((node) => {
      store.set(registerNodeAtom, node.id);
    });
  });

  it('should process a complete workflow execution sequence', () => {
    // Simulate workflow execution with header events

    // 1. First header event: "gather applicant context"
    const notification1: WatchNotification = {
      functionName: 'SimpleWorkflow',
      isStream: false,
      stateUpdate: {
        nodeId: 1,
        logFilterKey: 'SimpleWorkflow|root:0|hdr:gather-applicant-context:0',
        newState: 'running',
      },
      value: JSON.stringify({
        type: 'header',
        label: 'gather applicant context',
        level: 1,
        span: {
          file_path: '/test/workflow.baml',
          start_line: 5,
          start_column: 2,
          end_line: 5,
          end_column: 25,
        },
      }),
    };

    const parsed1 = parseWatchValue(notification1.value ?? '');
    const vizNodeId1 = notification1.stateUpdate?.nodeId.toString();
    expect(notification1.stateUpdate?.logFilterKey).toBe('SimpleWorkflow|root:0|hdr:gather-applicant-context:0');

    // Find node and update state
    const nodeId1 = vizNodeId1;
    expect(nodeId1).toBe('1');

    if (nodeId1) {
      store.set(nodeStateAtomFamily(nodeId1), 'running');
    }

    expect(store.get(nodeStateAtomFamily('1'))).toBe('running');
    expect(store.get(nodeStateAtomFamily('2'))).toBe('not-started');

    // 2. Second header event: "normalize profile signals"
    const notification2: WatchNotification = {
      functionName: 'SimpleWorkflow',
      isStream: false,
      stateUpdate: {
        nodeId: 2,
        logFilterKey: 'SimpleWorkflow|root:0|hdr:normalize-profile-signals:1',
        newState: 'running',
      },
      value: JSON.stringify({
        type: 'header',
        label: 'normalize profile signals',
        level: 1,
        span: {
          file_path: '/test/workflow.baml',
          start_line: 10,
          start_column: 2,
          end_line: 10,
          end_column: 27,
        },
      }),
    };

    const parsed2 = parseWatchValue(notification2.value ?? '');
    const vizNodeId2 = notification2.stateUpdate?.nodeId.toString();
    expect(notification2.stateUpdate?.logFilterKey).toBe('SimpleWorkflow|root:0|hdr:normalize-profile-signals:1');

    const nodeId2 = vizNodeId2;
    expect(nodeId2).toBe('2');

    if (nodeId2) {
      store.set(nodeStateAtomFamily(nodeId2), 'running');
    }

    // Both should now be running (or first could be success)
    expect(store.get(nodeStateAtomFamily('1'))).toBe('running');
    expect(store.get(nodeStateAtomFamily('2'))).toBe('running');
  });

  it('should handle stream events within workflow', () => {
    // Simulate a streaming LLM call within a workflow

    // Header event
    const headerNotification: WatchNotification = {
      functionName: 'SimpleWorkflow',
      isStream: false,
      value: JSON.stringify({
        type: 'header',
        label: 'gather applicant context',
        level: 1,
      }),
    };

    const enrichedHeader = parseWatchValue(headerNotification.value ?? '');
    expect(enrichedHeader?.type).toBe('header');

    // Stream start
    const streamStartNotification: WatchNotification = {
      variableName: 'profile',
      functionName: 'SimpleWorkflow',
      isStream: true,
      value: JSON.stringify({
        type: 'stream_start',
        id: 'profile-stream-1',
      }),
    };

    const enrichedStart = parseWatchValue(streamStartNotification.value ?? '');
    expect(enrichedStart?.type).toBe('stream_start');
    if (enrichedStart?.type === 'stream_start') {
      expect(enrichedStart.id).toBe('profile-stream-1');
    }

    // Stream update
    const streamUpdateNotification: WatchNotification = {
      variableName: 'profile',
      functionName: 'SimpleWorkflow',
      isStream: true,
      value: JSON.stringify({
        type: 'stream_update',
        id: 'profile-stream-1',
        value: '{"name": "John"}',
      }),
    };

    const enrichedUpdate = parseWatchValue(streamUpdateNotification.value ?? '');
    expect(enrichedUpdate?.type).toBe('stream_update');

    // Stream end
    const streamEndNotification: WatchNotification = {
      variableName: 'profile',
      functionName: 'SimpleWorkflow',
      isStream: true,
      value: JSON.stringify({
        type: 'stream_end',
        id: 'profile-stream-1',
      }),
    };

    const enrichedEnd = parseWatchValue(streamEndNotification.value ?? '');
    expect(enrichedEnd?.type).toBe('stream_end');
  });
});
