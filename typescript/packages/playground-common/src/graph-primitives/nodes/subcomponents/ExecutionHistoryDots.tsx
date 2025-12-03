import { useAtomValue } from 'jotai';
import { memo, useMemo } from 'react';

import { activeWorkflowExecutionsAtom } from '../../../sdk/atoms/core.atoms';

interface ExecutionHistoryDotsProps {
  nodeId: string;
}

export const ExecutionHistoryDots = memo(({ nodeId }: ExecutionHistoryDotsProps) => {
  const executions = useAtomValue(activeWorkflowExecutionsAtom);

  // Get execution history for this node (last 10)
  const history = useMemo(() => {
    const nodeHistory = executions
      .map((execution) => {
        const nodeExec = execution.nodeExecutions.get(nodeId);
        if (!nodeExec) return null;

        return {
          executionId: execution.id,
          state: nodeExec.state,
          timestamp: execution.timestamp,
        };
      })
      .filter((h): h is NonNullable<typeof h> => h !== null)
      .slice(-10); // Take last 10

    return nodeHistory;
  }, [executions, nodeId]);

  if (history.length === 0) return null;

  // Map state to color
  const getStateColor = (state: string) => {
    switch (state) {
      case 'success':
        return 'bg-green-500 dark:bg-green-400';
      case 'error':
        return 'bg-red-500 dark:bg-red-400';
      case 'running':
        return 'bg-blue-500 dark:bg-blue-400 animate-pulse';
      case 'pending':
        return 'bg-yellow-500 dark:bg-yellow-400';
      case 'skipped':
        return 'bg-gray-400 dark:bg-gray-500';
      case 'cached':
        return 'bg-purple-500 dark:bg-purple-400';
      default:
        return 'bg-gray-300 dark:bg-gray-600';
    }
  };

  return (
    <div className="flex gap-0.5 mt-1 min-h-[4px]">
      {history.map((h) => (
        <div
          key={h.executionId}
          className={`w-1 h-1 rounded-full ${getStateColor(h.state)}`}
          title={`Execution: ${h.state}`}
        />
      ))}
    </div>
  );
});

ExecutionHistoryDots.displayName = 'ExecutionHistoryDots';
