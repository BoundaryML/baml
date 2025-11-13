/**
 * Workflow Toolbar Component
 *
 * Top toolbar for workflow selection and graph controls
 */

import { useWorkflows, useActiveWorkflow, useLayoutDirection, useBAMLSDK } from '../../../sdk/hooks';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@baml/ui/select';

interface WorkflowToolbarProps {
  isDarkMode: boolean;
  onToggleDarkMode: () => void;
  onResetLayout: () => void;
}

export function WorkflowToolbar({ isDarkMode, onToggleDarkMode, onResetLayout }: WorkflowToolbarProps) {
  const workflows = useWorkflows();
  const { activeWorkflow, setActiveWorkflow } = useActiveWorkflow();
  const [direction, setDirection] = useLayoutDirection();
  const sdk = useBAMLSDK();

  const handleRunWorkflow = async () => {
    if (!activeWorkflow) return;

    console.log('▶️ Running workflow:', activeWorkflow.id);

    try {
      const executionId = await sdk.executions.start(
        activeWorkflow.id,
        { input: 'test input' },
        { clearCache: true }
      );

      console.log('✅ Started execution:', executionId);
    } catch (error) {
      console.error('❌ Failed to start execution:', error);
    }
  };

  return (
    <div className="absolute top-2 right-2 z-[1000] flex gap-1 bg-card p-1 rounded-lg shadow-md border text-[10px]">
      {/* Workflow Selector Dropdown */}
      <Select value={activeWorkflow?.id} onValueChange={setActiveWorkflow}>
        <SelectTrigger className="h-6 text-[10px] px-2 w-[140px]">
          <SelectValue placeholder="Select workflow" />
        </SelectTrigger>
        <SelectContent>
          {workflows.map((workflow) => (
            <SelectItem
              key={workflow.id}
              value={workflow.id}
              className="text-[10px]"
            >
              {workflow.displayName}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* Run Button */}
      <div className="border-l pl-1 ml-0.5 flex items-center gap-1">
        <button
          onClick={handleRunWorkflow}
          disabled={!activeWorkflow}
          className="px-2 py-0.5 h-6 rounded text-[10px] font-medium bg-green-600 text-white hover:bg-green-700 disabled:opacity-50 disabled:cursor-not-allowed transition-all"
          title="Run workflow"
        >
          ▶ Run
        </button>
      </div>

      {/* Direction Toggle */}
      <div className="border-l pl-1 ml-0.5 flex items-center gap-0.5">
        <button
          onClick={() => setDirection(direction === 'vertical' ? 'horizontal' : 'vertical')}
          className="p-0.5 h-6 w-6 rounded bg-secondary text-secondary-foreground hover:bg-secondary/80 transition-all"
          title={`Switch to ${direction === 'vertical' ? 'horizontal' : 'vertical'} layout`}
        >
          {direction === 'vertical' ? (
            <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
            </svg>
          ) : (
            <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7h12M8 12h12m-12 5h12M4 7h.01M4 12h.01M4 17h.01" />
            </svg>
          )}
        </button>

        {/* Reset Layout Button */}
        <button
          onClick={onResetLayout}
          className="p-0.5 h-6 w-6 rounded bg-secondary text-secondary-foreground hover:bg-secondary/80 transition-all"
          title="Reset layout (re-run ELK)"
        >
          <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
        </button>
      </div>

      {/* Dark Mode Toggle */}
      <div className="border-l pl-1 ml-0.5 flex items-center">
        <button
          onClick={onToggleDarkMode}
          className="p-0.5 h-6 w-6 rounded bg-secondary text-secondary-foreground hover:bg-secondary/80 transition-all"
          title={isDarkMode ? 'Switch to light mode' : 'Switch to dark mode'}
        >
          {isDarkMode ? (
            <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
            </svg>
          ) : (
            <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
            </svg>
          )}
        </button>
      </div>
    </div>
  );
}
