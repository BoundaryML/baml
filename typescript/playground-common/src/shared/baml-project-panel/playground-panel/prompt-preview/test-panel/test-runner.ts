import type { WasmFunctionResponse } from '@gloo-ai/baml-schema-wasm-web'
import { useAtomValue, useSetAtom } from 'jotai'
import { findMediaFile } from '../media-utils'
import { ctxAtom, runtimeAtom, wasmAtom } from '../../../atoms'
import { useAtomCallback } from 'jotai/utils'
import { useCallback } from 'react'
import {
  type TestState,
  testCaseAtom,
  areTestsRunningAtom,
  selectedTestcaseAtom,
  selectedFunctionAtom,
} from '../../atoms'
import { vscode } from '../../../vscode'
import { testHistoryAtom, selectedHistoryIndexAtom, type TestHistoryRun } from './atoms'
import { isClientCallGraphEnabledAtom } from '../../preview-toolbar'

export const useRunTests = (maxBatchSize = 5) => {
  const { rt } = useAtomValue(runtimeAtom)
  const ctx = useAtomValue(ctxAtom)
  const wasm = useAtomValue(wasmAtom)
  const setSelectedTestcase = useSetAtom(selectedTestcaseAtom)
  const setSelectedFunction = useSetAtom(selectedFunctionAtom)
  const setIsClientCallGraphEnabled = useSetAtom(isClientCallGraphEnabledAtom)
  const runTests = useAtomCallback(
    useCallback(
      async (get, set, tests: { functionName: string; testName: string }[]) => {
        // Create a new history run
        const historyRun: TestHistoryRun = {
          timestamp: Date.now(),
          tests: tests.map((test) => ({
            timestamp: Date.now(),
            functionName: test.functionName,
            testName: test.testName,
            response: { status: 'running' },
            input: get(testCaseAtom(test))?.tc.inputs, // Store input
          })),
        }

        setIsClientCallGraphEnabled(false)

        set(testHistoryAtom, (prev) => [historyRun, ...prev])
        set(selectedHistoryIndexAtom, 0)

        const setState = (test: { functionName: string; testName: string }, update: TestState) => {
          set(testHistoryAtom, (prev) => {
            const newHistory = [...prev]
            const currentRun = newHistory[0]
            if (!currentRun) return prev

            const testIndex = currentRun.tests.findIndex(
              (t) => t.functionName === test.functionName && t.testName === test.testName,
            )
            if (testIndex === -1) return prev

            const existingTest = currentRun.tests[testIndex]
            if (!existingTest) return prev

            currentRun.tests[testIndex] = {
              ...existingTest,
              response: update,
              timestamp: Date.now(),
              functionName: existingTest.functionName,
              testName: existingTest.testName,
            }
            return newHistory
          })
        }

        const runTest = async (test: { functionName: string; testName: string }) => {
          try {
            const testCase = get(testCaseAtom(test))
            console.log('test deps', testCase, rt, ctx, wasm)
            if (!rt || !ctx || !testCase || !wasm) {
              setState(test, { status: 'error', message: 'Missing required dependencies' })
              console.error('Missing required dependencies')
              return
            }

            const startTime = performance.now()
            setState(test, { status: 'running' })
            const result = await testCase.fn.run_test(
              rt,
              testCase.tc.name,
              vscode.loadEnv(),
              (partial: WasmFunctionResponse) => {
                setState(test, { status: 'running', response: partial })
              },
              findMediaFile,
              vscode.loadAwsCreds.bind(vscode),
            )
            console.log('result', result)

            const endTime = performance.now()
            const response_status = result.status()
            const responseStatusMap = {
              [wasm.TestStatus.Passed]: 'passed',
              [wasm.TestStatus.LLMFailure]: 'llm_failed',
              [wasm.TestStatus.ParseFailure]: 'parse_failed',
              [wasm.TestStatus.ConstraintsFailed]: 'constraints_failed',
              [wasm.TestStatus.AssertFailed]: 'assert_failed',
              [wasm.TestStatus.UnableToRun]: 'error',
              [wasm.TestStatus.FinishReasonFailed]: 'error',
            } as const

            setState(test, {
              status: 'done',
              response: result,
              response_status: responseStatusMap[response_status] || 'error',
              latency_ms: endTime - startTime,
            })
          } catch (e) {
            console.log('test error!')
            console.error(e)
            setState(test, {
              status: 'error',
              message: e instanceof Error ? e.message : 'Unknown error',
            })
          }
        }

        const run = async () => {
          // Create batches of tests to run
          const batches: { functionName: string; testName: string }[][] = []
          for (let i = 0; i < tests.length; i += maxBatchSize) {
            batches.push(tests.slice(i, i + maxBatchSize))
          }

          if (tests.length == 0) {
            console.error('No tests found')
            return
          }

          const firstTest = get(testCaseAtom(tests[0]))
          if (firstTest) {
            setSelectedFunction(firstTest.fn.name)
            setSelectedTestcase(firstTest.tc.name)
          } else {
            console.error("Invalid test found, so won't select this test case in the prompt preview", tests[0])
          }

          // Run each batch
          for (const batch of batches) {
            // TODO: parallelize when we fix wasm issues with runtime undefined after multiple runs
            for (const test of batch) {
              setState(test, { status: 'queued' })
              await runTest(test)
            }
          }
        }

        set(areTestsRunningAtom, true)
        await run().finally(() => {
          set(areTestsRunningAtom, false)
        })
      },
      [maxBatchSize, rt, ctx, wasm],
    ),
  )

  return { setRunningTests: runTests }
}
