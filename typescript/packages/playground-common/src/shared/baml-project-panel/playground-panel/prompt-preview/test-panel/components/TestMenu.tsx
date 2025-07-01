import { History, RefreshCw } from 'lucide-react'

import { useAtomValue } from 'jotai'
import { useAtom } from 'jotai'
import {  selectedHistoryIndexAtom, testHistoryAtom, TestHistoryEntry } from '../atoms'
import { useRunBamlTests } from '../test-runner'
import { ViewSelector } from './ViewSelector'
import { Tooltip, TooltipTrigger } from '@baml/ui/tooltip'
import { TooltipContent, TooltipProvider } from '@baml/ui/tooltip'
import { Button } from '@baml/ui/button'
import { Play } from 'lucide-react'
import { getStatus } from '../testStateUtils'

const getHistoryButtonColor = (tests: TestHistoryEntry[], isSelected: boolean) => {
  const baseClasses = isSelected
    ? {
        running: 'bg-blue-300 dark:bg-blue-700',
        error: 'bg-red-300 text-red-950 dark:bg-red-700 dark:text-red-50',
        success: 'bg-green-300 text-green-950 dark:bg-green-700 dark:text-green-50',
        warning: 'bg-yellow-300 text-yellow-950 dark:bg-yellow-700 dark:text-yellow-50',
        default: 'bg-gray-300 text-gray-950 dark:bg-gray-700 dark:text-gray-50',
      }
    : {
        running: 'border-blue-200 border hover:bg-blue-100 dark:border-blue-700/50 dark:hover:bg-blue-800/80',
        error: 'border-red-200 border hover:bg-red-100 dark:border-red-800/50 dark:hover:bg-red-900/80',
        success: 'border-green-200 border hover:bg-green-100 dark:border-green-700/50 dark:hover:bg-green-800/80',
        warning: 'border-yellow-200 border hover:bg-yellow-100 dark:border-yellow-700 dark:hover:bg-yellow-800',
        default: 'border-gray-200 border hover:bg-gray-100 dark:border-gray-700/50 dark:hover:bg-gray-800/80',
      }

  // Check if any test is running
  const hasRunning = tests.some((test) => test.response.status === 'running')
  if (hasRunning) {
    return baseClasses.running
  }

  // Check for errors first (highest priority)
  const hasError = tests.some((test) => {
    const status = test.response?.status
    const finalState = getStatus(test.response)
    return (
      status === 'error' ||
      (status === 'done' && ['parse_failed', 'llm_failed', 'assert_failed', 'error'].includes(finalState))
    )
  })
  if (hasError) {
    return baseClasses.error
  }

  // Then check for warnings (second priority)
  const hasWarning = tests.some((test) => {
    const status = test.response?.status
    const finalState = getStatus(test.response)
    return status === 'done' && finalState === 'constraints_failed'
  })
  if (hasWarning) {
    return baseClasses.warning
  }

  // If all tests are successful
  const allSuccess = tests.every((test) => {
    const status = test.response?.status
    const finalState = getStatus(test.response)
    return status === 'done' && finalState === 'passed'
  })
  if (allSuccess) {
    return baseClasses.success
  }

  // Default case
  return baseClasses.default
}

export const TestMenu = () => {
  const [selectedHistoryIndex, setSelectedHistoryIndex] = useAtom(selectedHistoryIndexAtom)
  const testHistory = useAtomValue(testHistoryAtom)
  const runBamlTests = useRunBamlTests()
  if (testHistory.length === 0) {
    return (
      <div className='flex justify-end items-center pr-2 mb-3 space-x-2'>
        <ViewSelector />
      </div>
    )
  }
  const currentRun = testHistory[selectedHistoryIndex]
  if (!currentRun)
    return (
      <div className='flex justify-end items-center pr-2 mb-3 space-x-2'>
        <ViewSelector />
      </div>
    )

  return (
    <div className='flex justify-between items-center pt-1 pr-2 mb-3'>
      <div className='flex items-center space-x-4'>
        <div className='flex overflow-x-auto'>
          <div className='flex gap-1 items-center'>
            <div className='flex gap-1 items-center pr-3'>
              <History className='w-4 h-4 opacity-50' />
            </div>
            {testHistory.slice(-7).map((run, index) => (
              <button
                key={index}
                onClick={() => setSelectedHistoryIndex(index)}
                className={`h-6 min-w-6 rounded px-1.5 text-xs ${getHistoryButtonColor(
                  run.tests,
                  selectedHistoryIndex === index,
                )}`}
              >
                {testHistory.length - index}
              </button>
            ))}
          </div>
        </div>
      </div>
      <div className='flex gap-2 items-center'>
        <TooltipProvider>
          <Tooltip delayDuration={0}>
            <TooltipTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='w-6 h-6'
                onClick={() => {
                  const allTests = currentRun.tests.map((test) => ({
                    functionName: test.functionName,
                    testName: test.testName,
                  }))
                  runBamlTests(allTests)
                }}
              >
                <Play className='w-4 h-4' fill='#a855f7' stroke='#a855f7' />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Re-run all tests</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>

        <TooltipProvider delayDuration={0}>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='w-6 h-6'
                onClick={() => {
                  const failedTests = currentRun.tests
                    .filter((test) => {
                      const status = (test.response as any).response_status
                      return (
                        status &&
                        ['parse_failed', 'llm_failed', 'assert_failed', 'error', 'constraints_failed'].includes(status)
                      )
                    })
                    .map((test) => ({
                      functionName: test.functionName,
                      testName: test.testName,
                    }))
                  if (failedTests.length > 0) {
                    runBamlTests(failedTests)
                  }
                }}
              >
                <RefreshCw className='w-4 h-4' />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Re-run failed tests</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>

        <ViewSelector />
      </div>
    </div>
  )
}
