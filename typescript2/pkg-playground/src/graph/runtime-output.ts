import type { BamlJsMedia, BamlJsValue } from '@b/pkg-proto';
import type { DeserializedRuntimeEvent } from '../worker-protocol';
import { findImageMedia } from '../shared/media-values';
import type { GraphNode, NodeExecutionState } from './types';

export interface GraphNodeOutput {
  result: BamlJsValue;
  imageOutputs: BamlJsMedia[];
}

export interface GraphNodeRuntime {
  result?: BamlJsValue | null;
  hasResult?: boolean;
  imageOutputs: BamlJsMedia[];
  executionState: NodeExecutionState;
  errorMessage?: string | null;
}

export interface GraphRunState {
  status?: 'running' | 'success' | 'error' | 'cancelled';
  error?: string | null;
  functionName?: string | null;
}

function nodeMatchesFunctionName(node: GraphNode, functionName: string): boolean {
  const label = node.label.trim();
  if (label === functionName) return true;
  if (label.startsWith(`${functionName}(`)) return true;

  const namespacedName = functionName.split('.').pop();
  return namespacedName != null
    && namespacedName !== functionName
    && (label === namespacedName || label.startsWith(`${namespacedName}(`));
}

function findOutputNodeIds(graphNodes: GraphNode[], functionName: string): string[] {
  const exact = graphNodes
    .filter((node) => node.label.trim() === functionName)
    .map((node) => node.id);
  if (exact.length > 0) return exact;

  return graphNodes
    .filter((node) => nodeMatchesFunctionName(node, functionName))
    .map((node) => node.id);
}

function findOutputNodeId(graphNodes: GraphNode[], functionName: string): string | null {
  return findOutputNodeIds(graphNodes, functionName)[0] ?? null;
}

const statePriority: Record<NodeExecutionState, number> = {
  'not-started': 0,
  skipped: 0,
  pending: 1,
  cached: 2,
  success: 3,
  cancelled: 4,
  running: 5,
  error: 6,
};

function mergeState(current: NodeExecutionState | undefined, next: NodeExecutionState): NodeExecutionState {
  if (current == null) return next;
  return statePriority[next] > statePriority[current] ? next : current;
}

function updateRuntime(
  map: Map<string, GraphNodeRuntime>,
  nodeId: string,
  patch: Partial<GraphNodeRuntime> & { executionState: NodeExecutionState },
) {
  const prev = map.get(nodeId);
  map.set(nodeId, {
    result: 'result' in patch ? patch.result : prev?.result,
    hasResult: 'hasResult' in patch ? patch.hasResult : prev?.hasResult,
    imageOutputs: 'imageOutputs' in patch ? (patch.imageOutputs ?? []) : prev?.imageOutputs ?? [],
    executionState: patch.executionState,
    errorMessage: 'errorMessage' in patch ? patch.errorMessage : prev?.errorMessage,
  });
}

function applySuccessfulRunFallback(
  map: Map<string, GraphNodeRuntime>,
  graphNodes: GraphNode[],
) {
  for (const node of graphNodes) {
    if (map.has(node.id)) continue;

    const hasSourceBackedRuntimeNode = node.parent == null || node.metadata.sourceExpr != null;
    if (!hasSourceBackedRuntimeNode) continue;

    updateRuntime(map, node.id, {
      executionState: 'success',
    });
  }
}

export function collectGraphNodeRuntime(
  graphNodes: GraphNode[],
  runtimeEvents: DeserializedRuntimeEvent[],
  runState?: GraphRunState,
): Map<string, GraphNodeRuntime> {
  const direct = new Map<string, GraphNodeRuntime>();
  const parentById = new Map(graphNodes.map((node) => [node.id, node.parent]));
  const activeNodeCounts = new Map<string, number>();
  const nodeIdBySpanId = new Map<string, string>();
  const nextCandidateByFunctionName = new Map<string, number>();

  function assignNodeForFunctionStart(functionName: string): string | null {
    const candidates = findOutputNodeIds(graphNodes, functionName);
    if (candidates.length === 0) return null;

    const nextIndex = nextCandidateByFunctionName.get(functionName) ?? 0;
    nextCandidateByFunctionName.set(functionName, nextIndex + 1);
    return candidates[nextIndex % candidates.length] ?? null;
  }

  function incrementActiveNodeCount(nodeId: string) {
    activeNodeCounts.set(nodeId, (activeNodeCounts.get(nodeId) ?? 0) + 1);
  }

  function decrementActiveNodeCount(nodeId: string): number {
    const nextCount = Math.max((activeNodeCounts.get(nodeId) ?? 0) - 1, 0);
    if (nextCount === 0) {
      activeNodeCounts.delete(nodeId);
    } else {
      activeNodeCounts.set(nodeId, nextCount);
    }
    return nextCount;
  }

  for (const evt of runtimeEvents) {
    const kind = evt.event;
    if (kind?.$case === 'functionStart') {
      const nodeId = assignNodeForFunctionStart(kind.functionStart.name);
      if (!nodeId) continue;

      nodeIdBySpanId.set(evt.spanId, nodeId);
      incrementActiveNodeCount(nodeId);
      updateRuntime(direct, nodeId, {
        executionState: 'running',
      });
    } else if (kind?.$case === 'functionEnd') {
      const mappedNodeId = nodeIdBySpanId.get(evt.spanId);
      const nodeId = mappedNodeId
        ?? findOutputNodeId(graphNodes, kind.functionEnd.name);
      if (mappedNodeId != null) {
        decrementActiveNodeCount(mappedNodeId);
      }
      nodeIdBySpanId.delete(evt.spanId);

      if (!nodeId) continue;
      const remainingActiveCount = activeNodeCounts.get(nodeId) ?? 0;
      const error = kind.functionEnd.error || null;
      updateRuntime(direct, nodeId, {
        result: kind.functionEnd.result,
        hasResult: kind.functionEnd.result != null,
        imageOutputs: kind.functionEnd.result == null ? [] : findImageMedia(kind.functionEnd.result),
        executionState: remainingActiveCount > 0 ? 'running' : error ? 'error' : 'success',
        errorMessage: error,
      });
    }
  }

  if (runState?.status === 'error' || runState?.status === 'cancelled') {
    const terminalState = runState.status;
    const activeNodeIds = [...activeNodeCounts.entries()]
      .filter(([, count]) => count > 0)
      .map(([nodeId]) => nodeId);
    const fallbackNodeId = runState.functionName
      ? findOutputNodeId(graphNodes, runState.functionName)
      : null;
    const errorTargets = activeNodeIds.length > 0
      ? activeNodeIds
      : fallbackNodeId != null && (activeNodeCounts.get(fallbackNodeId) ?? 0) === 0
        ? [fallbackNodeId]
        : [];

    for (const nodeId of errorTargets) {
      updateRuntime(direct, nodeId, {
        executionState: terminalState,
        errorMessage: runState.error,
      });
    }
  }

  if (runState?.status === 'success') {
    applySuccessfulRunFallback(direct, graphNodes);
  }

  const withAncestors = new Map(direct);
  for (const [nodeId, runtime] of direct) {
    let parentId = parentById.get(nodeId);
    while (parentId != null) {
      const prev = withAncestors.get(parentId);
      withAncestors.set(parentId, {
        result: runtime.result ?? prev?.result,
        hasResult: Boolean(prev?.hasResult || runtime.hasResult),
        imageOutputs: [
          ...(prev?.imageOutputs ?? []),
          ...(runtime.imageOutputs ?? []),
        ],
        executionState: mergeState(prev?.executionState, runtime.executionState),
        errorMessage: runtime.errorMessage ?? prev?.errorMessage,
      });
      parentId = parentById.get(parentId);
    }
  }

  return withAncestors;
}

export function collectGraphNodeOutputs(
  graphNodes: GraphNode[],
  runtimeEvents: DeserializedRuntimeEvent[],
): Map<string, GraphNodeOutput> {
  const runtime = collectGraphNodeRuntime(graphNodes, runtimeEvents);
  const outputs = new Map<string, GraphNodeOutput>();

  for (const [nodeId, nodeRuntime] of runtime) {
    if (nodeRuntime.result == null) continue;
    outputs.set(nodeId, {
      result: nodeRuntime.result,
      imageOutputs: nodeRuntime.imageOutputs,
    });
  }

  return outputs;
}
