/**
 * Execution State Sync Hook
 *
 * Synchronizes execution state from the SDK to ReactFlow nodes.
 * Updates node visual states and preserves outputs/errors.
 *
 * Smart State Management:
 * - Nodes that RAN in latest execution: show their state (dimmed when complete)
 * - Nodes that DIDN'T run: reset to 'not-started' but preserve outputs
 */

import { useEffect } from 'react';
import { useReactFlow } from '@xyflow/react';
import { useAtomValue } from 'jotai';
import {
  useAllNodeStates,
  useNodeExecutions,
  useLatestExecution,
} from '../../../sdk/hooks';
import { selectedNodeIdAtom } from '../../../sdk/atoms/core.atoms';

/**
 * Hook that syncs execution state to ReactFlow nodes
 */
export function useExecutionSync() {
  const { setNodes } = useReactFlow();
  const nodeStates = useAllNodeStates();
  const nodeExecutions = useNodeExecutions();
  const latestExecution = useLatestExecution();
  const selectedNodeId = useAtomValue(selectedNodeIdAtom);

  useEffect(() => {
    const isExecutionRunning = latestExecution?.status === 'running';

    setNodes((currentNodes) =>
      currentNodes.map((node) => {
        const state = nodeStates.get(node.id);

        // If no state for this node, reset execution state but preserve outputs
        // This ensures nodes that didn't run in the latest execution show their
        // last known data but without active/dimmed state indicators
        if (!state) {
          return {
            ...node,
            selected: node.id === selectedNodeId,
            data: {
              ...node.data,
              executionState: 'not-started',
              isExecutionActive: false,
              // Keep previous outputs/errors so users can see last known data
            },
          };
        }

        // Get execution data for this node
        const execution = nodeExecutions.get(node.id);

        // Update node data with execution state, outputs, and errors
        return {
          ...node,
          selected: node.id === selectedNodeId,
          data: {
            ...node.data,
            executionState: state,
            isExecutionActive: isExecutionRunning,
            outputs: execution?.outputs,
            error: execution?.error,
          },
        };
      })
    );
  }, [
    nodeStates,
    nodeExecutions,
    latestExecution?.status,
    selectedNodeId,
    setNodes,
  ]);
}
