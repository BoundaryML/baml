/* eslint-disable @typescript-eslint/no-floating-promises */
import { useAtomValue } from 'jotai'
import { Play, Square } from 'lucide-react'
import { useEffect, useRef, useCallback } from 'react'
import { Button } from '@baml/ui/button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@baml/ui/tooltip'
import { cn } from '@baml/ui/lib/utils'
import { testCaseResponseAtom, type TestState, testcaseObjectAtom } from '../../../atoms'
import { FunctionTestName } from '../../../function-test-name'
import { type TestHistoryRun } from '../atoms'
import { useRunBamlTests } from '../test-runner'
import { getStatus } from '../testStateUtils'
import { ResponseRenderer } from './ResponseRenderer'
import { TestStatus } from './TestStatus'
import { EnhancedErrorRenderer } from './EnhancedErrorRenderer'
import { useNavigation } from '../../../../../../sdk/hooks'
import { unifiedSelectionStateAtom } from '../../../../../../sdk/atoms/core.atoms'

export const CardView = ({ currentRun }: { currentRun?: TestHistoryRun }) => {
  return (
    <div className='space-y-4'>
      {currentRun?.tests.map((test, index) => (
        <TestResult
          key={index}
          testId={{
            functionName: test.functionName,
            testName: test.testName,
          }}
          historicalResponse={test.response}
        />
      ))}
    </div>
  )
}
interface TestId {
  functionName: string
  testName: string
}

interface TestResultProps {
  testId: TestId;
  historicalResponse?: TestState;
}

const TestResult = ({ testId, historicalResponse }: TestResultProps) => {
  const response = useAtomValue(testCaseResponseAtom(testId))
  const displayResponse = historicalResponse || response
  const { runTests: runBamlTests, cancelTests } = useRunBamlTests()
  const navigate = useNavigation()
  const selection = useAtomValue(unifiedSelectionStateAtom)
  const cardRef = useRef<HTMLDivElement>(null)

  const functionName = selection.mode === 'function' ? selection.functionName : selection.mode === 'workflow' ? selection.selectedNodeId : null;
  const testName = selection.mode === 'function' || selection.mode === 'workflow' ? selection.testName : selection.mode === 'loading' ? selection.intent.testName ?? null : null;
  const isSelected = functionName === testId.functionName && testName === testId.testName
  const isThisTestRunning = displayResponse?.status === 'running'

  useEffect(() => {
    if (isSelected && cardRef.current) {
      cardRef.current.scrollIntoView({
        behavior: 'smooth',
        block: 'nearest',
      })
    }
  }, [isSelected, displayResponse?.status])

  if (!displayResponse) {
    console.log('no display response')
    return null
  }

  // Use useCallback to create a stable retry function
  const handleRetry = useCallback(() => {
    runBamlTests([{ functionName: testId.functionName, testName: testId.testName }]);
  }, [runBamlTests, testId.functionName, testId.testName]);

  return (
    <div
      ref={cardRef}
      className={cn(
        'flex cursor-pointer flex-col gap-2 rounded-lg border p-3 transition-colors hover:bg-muted/70 dark:bg-muted/20',
        isSelected && 'border-purple-500/20 shadow-sm dark:border-purple-900/30 dark:bg-muted/90',
      )}
      onClick={() => {
        navigate({
          kind: 'test',
          functionName: testId.functionName,
          testName: testId.testName,
          source: 'test-panel',
          timestamp: Date.now(),
        });
      }}
    >
      <div className='flex gap-2 justify-between items-center'>
        <div className='flex gap-2 items-center'>
          <TooltipProvider>
            <Tooltip >
              <TooltipTrigger asChild>
                <Button
                  variant='ghost'
                  size='icon'
                  className='w-6 h-6 shrink-0'
                  onClick={(e) => {

                    e.stopPropagation()
                    try {
                      if (isThisTestRunning) {
                        cancelTests()
                      } else {
                        runBamlTests([testId])
                      }
                    } catch (error) {
                      console.error('Error running test', error);
                      throw error;
                    }
                  }}
                >
                  {isThisTestRunning ? (
                    <Square className='w-4 h-4 fill-red-500 stroke-red-500' />
                  ) : (
                    <Play className='w-4 h-4' fill='#a855f7' stroke='#a855f7' />
                  )}
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                <p>{isThisTestRunning ? 'Stop test' : 'Re-run test'}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
          <FunctionTestName functionName={testId.functionName} testName={testId.testName} selected={isSelected} />
        </div>
        <TestStatus status={displayResponse.status} finalState={getStatus(displayResponse)} />
      </div>

      {displayResponse.status === 'running' && typeof displayResponse.response === 'object' && <ResponseRenderer response={displayResponse.response} test={displayResponse} />}

      {displayResponse.status === 'done' && (
        <ResponseRenderer response={displayResponse.response} status={displayResponse.response_status} test={displayResponse} />
      )}

      {displayResponse.status === 'error' && (
        <EnhancedErrorRenderer
          errorMessage={displayResponse.message || 'Unknown error occurred'}
          functionName={testId.functionName}
          testName={testId.testName}
          onRetry={handleRetry}
        />
      )}
    </div>
  )
}
