import type { ReactFlowInstance } from '@xyflow/react';
import { createStore } from 'zenbox';

import type {
  ReactflowEdgeWithData,
  ReactflowNodeWithData,
} from '../mock-data/types';

const kInstance = null as unknown as ReactFlowInstance;

export const flowStore = createStore({
  ...kInstance,
  initialized: false,
  getNodesAndEdges: () => {
    const nodes = (flowStore.value.getNodes() ?? []) as ReactflowNodeWithData[];
    const edges = (flowStore.value.getEdges() ?? []) as ReactflowEdgeWithData[];
    return {
      nodes,
      edges,
      nodesMap: Object.fromEntries(nodes.map((v) => [v.id, v])),
      edgesMap: Object.fromEntries(edges.map((v) => [v.id, v])),
    };
  },
});
