import { SidebarMenuButton } from '@baml/ui/sidebar';
import { Tooltip, TooltipContent, TooltipTrigger } from '@baml/ui/tooltip';
import { useAtomValue } from 'jotai';
import { CheckCircle2, FunctionSquare, XCircle } from 'lucide-react';
import * as React from 'react';
import { vscode } from '../../vscode';
import { functionObjectAtom } from '../atoms';
import { Loader } from '../prompt-preview/components';
import {
  selectedHistoryIndexAtom,
  testHistoryAtom,
} from '../prompt-preview/test-panel/atoms';
import { getStatus } from '../prompt-preview/test-panel/testStateUtils';

interface FunctionItemProps {
  functionName: string;
  tests: string[];
}

export function FunctionItem({ functionName, tests }: FunctionItemProps) {
  const fnAtom = React.useMemo(
    () => functionObjectAtom(functionName),
    [functionName],
  );
  const fn = useAtomValue(fnAtom);

  const testHistory = useAtomValue(testHistoryAtom);
  const selectedIndex = useAtomValue(selectedHistoryIndexAtom);
  const currentRun = testHistory[selectedIndex];

  const functionTestsStatus = React.useMemo(() => {
    if (!currentRun) {
      return {
        hasRunning: false,
        hasTests: false,
        allPassed: false,
        anyFailed: false,
        passedCount: 0,
        failedCount: 0,
        totalCount: tests.length,
        lastRunTime: null,
      };
    }

    const functionTests = currentRun.tests.filter(
      (test) => test.functionName === functionName,
    );

    if (functionTests.length === 0) {
      return {
        hasRunning: false,
        hasTests: false,
        allPassed: false,
        anyFailed: false,
        passedCount: 0,
        failedCount: 0,
        totalCount: tests.length,
        lastRunTime: null,
      };
    }

    // Use the same logic as TestItem - check each test's status directly
    let hasRunning = false;
    let passedCount = 0;
    let failedCount = 0;

    for (const test of functionTests) {
      const status = test.response.status;

      if (status === 'running') {
        hasRunning = true;
        continue;
      }

      if (status === 'error') {
        failedCount++;
        continue;
      }

      if (status === 'done') {
        const finalState = getStatus(test.response);
        if (finalState === 'passed') {
          passedCount++;
        } else if (
          finalState === 'llm_failed' ||
          finalState === 'parse_failed' ||
          finalState === 'constraints_failed' ||
          finalState === 'assert_failed' ||
          finalState === 'error'
        ) {
          failedCount++;
        }
      }
      // For any other status (pending, etc.), we don't count it as passed or failed
    }

    if (hasRunning) {
      return {
        hasRunning: true,
        hasTests: true,
        allPassed: false,
        anyFailed: false,
        passedCount: 0,
        failedCount: 0,
        totalCount: tests.length,
        lastRunTime: currentRun.timestamp,
      };
    }

    // Only show status icons if we have some completed tests
    const totalProcessed = passedCount + failedCount;
    if (totalProcessed === 0) {
      // No tests have completed yet
      return {
        hasRunning: false,
        hasTests: true,
        allPassed: false,
        anyFailed: false,
        passedCount: 0,
        failedCount: 0,
        totalCount: tests.length,
        lastRunTime: currentRun.timestamp,
      };
    }

    // Check if all completed tests passed (not all tests in the function)
    const allPassed =
      totalProcessed > 0 && passedCount === totalProcessed && failedCount === 0;
    const anyFailed = failedCount > 0;

    return {
      hasRunning: false,
      hasTests: true,
      allPassed,
      anyFailed,
      passedCount,
      failedCount,
      totalCount: tests.length,
      lastRunTime: currentRun.timestamp,
    };
  }, [currentRun?.tests, functionName, tests.length]);

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (fn?.span) {
      vscode.jumpToFile(fn.span);
    }
  };

  return (
    <SidebarMenuButton
      className="flex justify-between items-center w-full pl-8 cursor-pointer"
      onClick={handleClick}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <div className="flex items-center gap-2 truncate cursor-pointer">
            {/* {functionTestsStatus.hasRunning ? (
              <Loader className="size-4" />
            ) : functionTestsStatus.allPassed ? (
              <CheckCircle2 className="size-4 text-green-500" />
            ) : functionTestsStatus.anyFailed ? (
              <XCircle className="size-4 text-red-500" />
            ) : (
              <FunctionSquare className="size-4" />
            )} */}
            <FunctionSquare className="size-4" />
            <span className="truncate hover:text-primary hover:underline">
              {functionName}
            </span>
          </div>
        </TooltipTrigger>
        <TooltipContent className="max-w-xs">
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <FunctionSquare className="size-4" />
              <span className="font-medium">{functionName}</span>
            </div>

            {functionTestsStatus.hasTests ? (
              <div className="space-y-1 text-xs">
                <div className="flex justify-between items-center">
                  <span className="text-muted-foreground">Tests:</span>
                  <span>{functionTestsStatus.totalCount}</span>
                </div>



                {functionTestsStatus.hasRunning ? (
                  <div className="flex items-center gap-1 text-blue-500">
                    <Loader className="size-3" />
                    <span>Running tests...</span>
                  </div>
                ) : functionTestsStatus.passedCount > 0 ||
                  functionTestsStatus.failedCount > 0 ? (
                  <>
                    {functionTestsStatus.passedCount > 0 && (
                      <div className="flex justify-between items-center">
                        <span className="text-green-500">Passed:</span>
                        <span className="text-green-500">
                          {functionTestsStatus.passedCount}
                        </span>
                      </div>
                    )}
                    {functionTestsStatus.failedCount > 0 && (
                      <div className="flex justify-between items-center">
                        <span className="text-red-500">Failed:</span>
                        <span className="text-red-500">
                          {functionTestsStatus.failedCount}
                        </span>
                      </div>
                    )}
                  </>
                ) : (
                  <div className="text-muted-foreground">No recent runs</div>
                )}

                {functionTestsStatus.lastRunTime && (
                  <div className="flex justify-between items-center pt-1 border-t border-border">
                    <span className="text-muted-foreground">Last run:</span>
                    <span className="text-muted-foreground">
                      {new Date(
                        functionTestsStatus.lastRunTime,
                      ).toLocaleTimeString()}
                    </span>
                  </div>
                )}
              </div>
            ) : (
              <div className="text-xs text-muted-foreground">
                {tests.length === 0
                  ? 'No tests defined'
                  : 'Click to navigate to function'}
              </div>
            )}
          </div>
        </TooltipContent>
      </Tooltip>
    </SidebarMenuButton>
  );
}
