import { nextTick } from '@del-wang/utils/web';
import { useState, useRef } from 'react';

import { flowStore } from '../../../states/reactflow';
import { type ILayoutReactflow, layoutReactflow } from './node';

const layoutWithFlush = async (options: ILayoutReactflow) => {
  const layout = await layoutReactflow(options);

  // Wait for the nodes and edges to be cleared
  flowStore.value.setNodes([]);
  flowStore.value.setEdges([]);
  while (flowStore.value.getNodes().length > 0) {
    await nextTick(3);
  }

  // Wait for the nodes and edges to be measured
  flowStore.value.setNodes(layout.nodes);
  flowStore.value.setEdges(layout.edges);
  while (!flowStore.value.getNodes()[0]?.measured) {
    await nextTick(3);
  }

  // Get layouted nodes and edges
  const { nodes, edges } = flowStore.value.getNodesAndEdges();


  return { layout, nodes, edges };
};

export const useAutoLayout = () => {
  const [isDirty, setIsDirty] = useState(false);
  const pendingLayoutRef = useRef<(ILayoutReactflow & { skipFitView?: boolean }) | null>(null);
  const isProcessingQueueRef = useRef(false);

  const processLayoutQueue = async () => {
    if (isProcessingQueueRef.current || !pendingLayoutRef.current) {
      return;
    }

    isProcessingQueueRef.current = true;

    while (pendingLayoutRef.current) {
      const options = pendingLayoutRef.current;
      pendingLayoutRef.current = null; // Clear before processing so new requests can queue

      if (!flowStore.value.initialized || options.nodes.length < 1) {

        continue;
      }

      setIsDirty(true);

      try {
        // Perform the first layout to measure node sizes
        const firstLayout = await layoutWithFlush({
          ...options,
          visibility: 'hidden', // Hide layout during the first layout pass
        });

        // Check if a newer layout was queued while we were processing
        if (pendingLayoutRef.current) {
          console.log('useAutoLayout: newer layout queued, skipping second pass');
          continue;
        }

        // Perform the second layout using actual node sizes
        const secondLayout = await layoutWithFlush({
          visibility: 'visible',
          ...options,
          nodes: firstLayout.nodes,
          edges: firstLayout.edges,
        });

        // Center the viewpoint only if skipFitView is not true
        if (!options.skipFitView) {
          await flowStore.value.fitView({ duration: 0 });
          await flowStore.value.zoomTo(flowStore.value.getZoom() * 0.8);
        }
      } catch (error) {
        console.error('aaron: useAutoLayout: layout error:', error);
      }
    }

    setIsDirty(false);
    isProcessingQueueRef.current = false;
  };

  const layout = async (options: ILayoutReactflow & { skipFitView?: boolean }) => {
    if (!flowStore.value.initialized || options.nodes.length < 1) {
      console.log('useAutoLayout: skipping layout:', {
        isDirty,
        flowStoreInitialized: flowStore.value.initialized,
      });
      return;
    }

    // Queue the layout request (overwrites any pending request with latest data)
    pendingLayoutRef.current = options;

    // Start processing if not already running
    void processLayoutQueue();
  };

  return { layout, isDirty };
};
