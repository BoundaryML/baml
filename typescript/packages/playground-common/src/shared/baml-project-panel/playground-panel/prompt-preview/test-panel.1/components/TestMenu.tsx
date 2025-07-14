import { Button } from '@baml/ui/button';
import { Tooltip, TooltipTrigger } from '@baml/ui/tooltip';
import { TooltipContent, TooltipProvider } from '@baml/ui/tooltip';
import { useAtomValue } from 'jotai';
import { useAtom } from 'jotai';
import { Play } from 'lucide-react';
import { selectedHistoryIndexAtom, testHistoryAtom } from '../atoms';
import { useRunBamlTests } from '../test-runner';
import { ViewSelector } from './ViewSelector';

export const TestMenu = () => {
  const [selectedHistoryIndex] = useAtom(selectedHistoryIndexAtom);
  const testHistory = useAtomValue(testHistoryAtom);
  const runBamlTests = useRunBamlTests();

  if (testHistory.length === 0) {
    return (
      <div className="flex justify-end items-center pr-2 mb-3 space-x-2">
        <ViewSelector />
      </div>
    );
  }

  const currentRun = testHistory[selectedHistoryIndex];
  if (!currentRun) {
    return (
      <div className="flex justify-end items-center pr-2 mb-3 space-x-2">
        <ViewSelector />
      </div>
    );
  }

  return (
    <div className="flex justify-end items-center pt-1 pr-2 mb-3">
      <div className="flex gap-2 items-center">
        <TooltipProvider>
          <Tooltip delayDuration={0}>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="w-6 h-6"
                onClick={() => {
                  const allTests = currentRun.tests.map((test) => ({
                    functionName: test.functionName,
                    testName: test.testName,
                  }));
                  runBamlTests(allTests);
                }}
              >
                <Play className="w-4 h-4" fill="#a855f7" stroke="#a855f7" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Re-run all tests</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
        <ViewSelector />
      </div>
    </div>
  );
};
