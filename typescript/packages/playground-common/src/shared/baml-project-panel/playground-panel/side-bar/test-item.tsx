import {
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@baml/ui/sidebar';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { useAtomValue } from 'jotai';
import {
  AlertTriangle,
  CheckCircle2,
  FlaskConical,
  Play,
  Square,
  XCircle,
} from 'lucide-react';
import type * as React from 'react';
import { useMemo } from 'react';
import { vscode } from '../../vscode';
import { testcaseObjectAtom } from '../atoms';
import { Loader } from '../prompt-preview/components';
import {
  selectedHistoryIndexAtom,
  testHistoryAtom,
} from '../prompt-preview/test-panel/atoms';
import { useRunBamlTests } from '../prompt-preview/test-panel/test-runner';
import { getStatus } from '../prompt-preview/test-panel/testStateUtils';
import type { TestItemProps } from './types';
import { highlightText } from './utils';
import { useNavigation } from '../../../../sdk/hooks';

const createSpan = (span: {
  start: number;
  end: number;
  file_path: string;
  start_line: number;
}) => ({
  start: span.start,
  end: span.end,
  source_file: span.file_path,
  value: `${span.file_path.split('/').pop() ?? '<file>.baml'}:${span.start_line + 1}`,
});

export function TestItem({
  label,
  isSelected = false,
  searchTerm = '',
  functionName,
}: TestItemProps) {
  const testHistory = useAtomValue(testHistoryAtom);
  const selectedIndex = useAtomValue(selectedHistoryIndexAtom);
  const { runTests: runBamlTests, cancelTests } = useRunBamlTests();
  const navigate = useNavigation();

  const testAtom = useMemo(
    () => testcaseObjectAtom({ functionName, testcaseName: label }),
    [functionName, label],
  );
  const tc = useAtomValue(testAtom);

  const currentRun = testHistory[selectedIndex];
  const testResult = currentRun?.tests.find(
    (t) => t.functionName === functionName && t.testName === label,
  );

  // Only show stop button if THIS specific test is running or queued
  const isThisTestRunning = testResult?.response.status === 'running' || testResult?.response.status === 'queued';

  const getStatusIcon = () => {
    if (!testResult) return <FlaskConical className="size-3" />;
    const status = testResult.response.status;
    const finalState = getStatus(testResult.response);
    if (status === 'running') return <Loader className="size-3" />;
    if (status === 'error') return <XCircle className="size-3 text-red-500" />;
    if (status === 'done') {
      if (finalState === 'passed')
        return <CheckCircle2 className="size-3 text-green-500" />;
      if (finalState === 'constraints_failed')
        return <AlertTriangle className="size-3 text-yellow-500" />;
      return <XCircle className="size-3 text-red-500" />;
    }
    return <FlaskConical className="size-3" />;
  };

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();

    // Navigate to test (same as DebugPanel)
    navigate({
      kind: 'test',
      functionName,
      testName: label,
      source: 'sidebar',
      timestamp: Date.now(),
    });
  };

  const handleJumpToFile = (e: React.MouseEvent) => {
    e.stopPropagation();

    // Navigate to test first
    navigate({
      kind: 'test',
      functionName,
      testName: label,
      source: 'sidebar',
      timestamp: Date.now(),
    });

    // Then jump to file
    if (tc?.span) {
      vscode.jumpToFile(tc.span);
    }
  };

  const handleRunTest = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isThisTestRunning) {
      cancelTests();
    } else {
      runBamlTests([{ functionName, testName: label }]);
    }
  };

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        onClick={handleClick}
        isActive={isSelected}
        className={`flex justify-between items-center w-full text-[10px] py-0.5 h-6 ${
          isSelected ? 'bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300' : ''
        }`}
      >
        <div className="flex items-center min-w-0 gap-1.5">
          {getStatusIcon()}
          <Tooltip delayDuration={500}>
            <TooltipTrigger asChild>
              <span
                className="truncate cursor-pointer hover:text-primary hover:underline"
                onClick={handleJumpToFile}
              >
                {highlightText(label, searchTerm)}
              </span>
            </TooltipTrigger>
            <TooltipContent className="max-w-xs">
              <div className="space-y-2">
                <div className="flex items-center gap-2">
                  {getStatusIcon()}
                  <span className="font-medium">{label}</span>
                </div>

                {testResult ? (
                  <div className="space-y-1 text-xs">
                    <div className="flex justify-between items-center">
                      <span className="text-muted-foreground">Status:</span>
                      <span className={`capitalize ${testResult.response.status === 'running' ? 'text-blue-500' :
                        testResult.response.status === 'error' ? 'text-red-500' :
                          getStatus(testResult.response) === 'passed' ? 'text-green-500' :
                            getStatus(testResult.response) === 'constraints_failed' ? 'text-yellow-500' :
                              'text-red-500'
                        }`}>
                        {testResult.response.status === 'done' ? getStatus(testResult.response) : testResult.response.status}
                      </span>
                    </div>

                    {/* Show checks/asserts information */}
                    {testResult.response.status === 'done' && testResult.response.response && (() => {
                      const parsedResponse = testResult.response.response.parsed_response;
                      const finalState = getStatus(testResult.response);
                      const checkCount = parsedResponse ? parsedResponse.checkCount : 0;

                      if (checkCount > 0 || finalState === 'constraints_failed' || finalState === 'assert_failed') {
                        return (
                          <div className="space-y-1">
                            {checkCount > 0 && (
                              <div className="flex justify-between items-center">
                                <span className="text-muted-foreground">Checks:</span>
                                <span className={finalState === 'constraints_failed' ? 'text-yellow-500' : 'text-green-500'}>
                                  {finalState === 'constraints_failed' ? `${checkCount} failed` : `${checkCount} passed`}
                                </span>
                              </div>
                            )}

                            {finalState === 'assert_failed' && (
                              <div className="flex justify-between items-center">
                                <span className="text-muted-foreground">Asserts:</span>
                                <span className="text-red-500">Failed</span>
                              </div>
                            )}
                          </div>
                        );
                      }
                      return null;
                    })()}

                    {testResult.response.status === 'done' && testResult.response.latency_ms && (
                      <div className="flex justify-between items-center">
                        <span className="text-muted-foreground">Duration:</span>
                        <span>{testResult.response.latency_ms.toFixed(0)}ms</span>
                      </div>
                    )}

                    {testResult.response.status === 'done' && testResult.response.response?.llm_response?.model && (
                      <div className="flex justify-between items-center">
                        <span className="text-muted-foreground">Model:</span>
                        <span className="truncate max-w-32">{testResult.response.response.llm_response?.model}</span>
                      </div>
                    )}

                    <div className="pt-1 border-t border-border text-muted-foreground">
                      Click to navigate to test
                    </div>
                  </div>
                ) : (
                  <div className="text-xs text-muted-foreground">
                    Click to navigate to test
                  </div>
                )}
              </div>
            </TooltipContent>
          </Tooltip>
        </div>
      </SidebarMenuButton>
      <SidebarMenuAction
        className="cursor-pointer size-[9px] items-center justify-center pt-0.5"
        onClick={handleRunTest}
      >
        {isThisTestRunning ? (
          <Square className="!size-3 fill-red-500 stroke-red-500" />
        ) : (
          <Play className="!size-3" />
        )}
      </SidebarMenuAction>
    </SidebarMenuItem>
  );
}
