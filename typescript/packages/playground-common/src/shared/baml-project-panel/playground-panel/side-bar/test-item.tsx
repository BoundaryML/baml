import {
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@baml/ui/sidebar';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { useAtomValue, useSetAtom } from 'jotai';
import {
  AlertTriangle,
  CheckCircle2,
  FlaskConical,
  Play,
  XCircle,
} from 'lucide-react';
import type * as React from 'react';
import { selectedItemAtom } from '../atoms';
import { Loader } from '../prompt-preview/components';
import {
  selectedHistoryIndexAtom,
  testHistoryAtom,
} from '../prompt-preview/test-panel/atoms';
import { useRunBamlTests } from '../prompt-preview/test-panel/test-runner';
import { getStatus } from '../prompt-preview/test-panel/testStateUtils';
import type { TestItemProps } from './types';
import { highlightText } from './utils';

export function TestItem({
  label,
  isSelected = false,
  searchTerm = '',
  functionName,
}: TestItemProps) {
  const testHistory = useAtomValue(testHistoryAtom);
  const selectedIndex = useAtomValue(selectedHistoryIndexAtom);
  const runBamlTests = useRunBamlTests();
  const setSelectedItem = useSetAtom(selectedItemAtom);

  const currentRun = testHistory[selectedIndex];
  const testResult = currentRun?.tests.find(
    (t) => t.functionName === functionName && t.testName === label,
  );

  const getStatusIcon = () => {
    if (!testResult) return <FlaskConical className="size-4" />;
    const status = testResult.response.status;
    const finalState = getStatus(testResult.response);
    if (status === 'running') return <Loader className="size-4" />;
    if (status === 'error') return <XCircle className="size-4 text-red-500" />;
    if (status === 'done') {
      if (finalState === 'passed')
        return <CheckCircle2 className="size-4 text-green-500" />;
      if (finalState === 'constraints_failed')
        return <AlertTriangle className="size-4 text-yellow-500" />;
      return <XCircle className="size-4 text-red-500" />;
    }
    return <FlaskConical className="size-4" />;
  };

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    setSelectedItem(functionName, label);
  };

  const handleRunTest = (e: React.MouseEvent) => {
    e.stopPropagation();
    runBamlTests([{ functionName, testName: label }]);
  };

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={handleClick}
        isActive={isSelected}
        className="flex justify-between items-center w-full"
      >
        <div className="flex items-center min-w-0">
          {getStatusIcon()}
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="ml-1 text-sm truncate">
                {highlightText(label, searchTerm)}
              </span>
            </TooltipTrigger>
            <TooltipContent>{label}</TooltipContent>
          </Tooltip>
        </div>
        <SidebarMenuAction
          onClick={handleRunTest}
          disabled={testResult?.response.status === 'running'}
        >
          <Play className="size-4" />
        </SidebarMenuAction>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
