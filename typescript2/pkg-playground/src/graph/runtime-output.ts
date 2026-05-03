import type { BamlJsMedia, BamlJsValue } from '@b/pkg-proto';
import type { DeserializedRuntimeEvent } from '../worker-protocol';
import { findImageMedia } from '../shared/media-values';
import type { GraphNode } from './types';

export interface GraphNodeOutput {
  result: BamlJsValue;
  imageOutputs: BamlJsMedia[];
}

function nodeMatchesFunctionName(node: GraphNode, functionName: string): boolean {
  const label = node.label.trim();
  if (label === functionName) return true;
  if (label.startsWith(`${functionName}(`)) return true;

  const namespacedName = functionName.split('.').pop();
  return namespacedName != null && namespacedName !== functionName && label.startsWith(`${namespacedName}(`);
}

function findOutputNodeId(graphNodes: GraphNode[], functionName: string): string | null {
  const exact = graphNodes.find((node) => node.label.trim() === functionName);
  if (exact) return exact.id;

  const call = graphNodes.find((node) => nodeMatchesFunctionName(node, functionName));
  return call?.id ?? null;
}

export function collectGraphNodeOutputs(
  graphNodes: GraphNode[],
  runtimeEvents: DeserializedRuntimeEvent[],
): Map<string, GraphNodeOutput> {
  const outputs = new Map<string, GraphNodeOutput>();

  for (const evt of runtimeEvents) {
    const kind = evt.event;
    if (kind?.$case !== 'functionEnd' || kind.functionEnd.result == null) continue;

    const nodeId = findOutputNodeId(graphNodes, kind.functionEnd.name);
    if (!nodeId) continue;

    outputs.set(nodeId, {
      result: kind.functionEnd.result,
      imageOutputs: findImageMedia(kind.functionEnd.result),
    });
  }

  return outputs;
}
