/**
 * Graph Sync Hook
 *
 * Converts SDK graph data to ReactFlow format and triggers layout.
 * Handles graph changes and ensures proper rendering.
 */

import { useEffect, useState, useRef } from 'react';
import { useAtomValue } from 'jotai';
import { convertedGraphAtom, currentGraphAtom, layoutDirectionAtom, selectedNodeIdAtom } from '../../../sdk/atoms/core.atoms';
import { useAutoLayout } from '../layout/useAutoLayout';

/**
 * Hook that converts SDK graph to ReactFlow and manages layout
 */
export function useGraphSync() {
  const currentGraph = useAtomValue(currentGraphAtom);
  const convertedGraph = useAtomValue(convertedGraphAtom);
  const direction = useAtomValue(layoutDirectionAtom);
  const { layout } = useAutoLayout();
  const [isLayoutLoading, setIsLayoutLoading] = useState(false);
  const lastLayoutKeyRef = useRef<string | null>(null);

  // Run layout when graph changes
  // Note: Only depend on convertedGraph (which includes direction in its memo)
  // to avoid infinite loops from layout function recreation
  useEffect(() => {
    if (!convertedGraph) return;

    const layoutKey = `${currentGraph.workflow?.id ?? 'standalone'}|${convertedGraph.nodes
      .map((node) => node.id)
      .join(',')}|${convertedGraph.edges.length}|${direction}`;

    if (lastLayoutKeyRef.current === layoutKey) {
      return;
    }
    lastLayoutKeyRef.current = layoutKey;

    console.log('📐 Running layout for', convertedGraph.nodes.length, 'nodes. Elk will run twice to measure nodes.');
    setIsLayoutLoading(true);

    // Add a safety timeout to prevent infinite loading state
    const timeoutId = setTimeout(() => {
      console.warn('⚠️ Layout calculation timed out after 5 seconds');
      setIsLayoutLoading(false);
    }, 5000);

    const layoutPromise = layout({
      nodes: convertedGraph.nodes,
      edges: convertedGraph.edges,
      direction,
    });

    if (layoutPromise) {
      layoutPromise
        .finally(() => {
          clearTimeout(timeoutId);
          setIsLayoutLoading(false);
        })
        .catch((error) => {
          console.error('❌ Layout calculation failed:', error);
          clearTimeout(timeoutId);
          setIsLayoutLoading(false);
        });
    } else {
      clearTimeout(timeoutId);
      setIsLayoutLoading(false);
    }

    return () => {
      clearTimeout(timeoutId);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [convertedGraph, direction]); // Re-run layout when graph or direction changes

  return {
    convertedGraph,
    isLayoutLoading,
  };
}
