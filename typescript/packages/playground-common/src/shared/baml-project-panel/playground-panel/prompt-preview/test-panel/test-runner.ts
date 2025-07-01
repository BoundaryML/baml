import type { WasmFunctionResponse, WasmSpan, WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web'
import { useAtomValue, useSetAtom } from 'jotai'
import { findMediaFile } from '../media-utils'
import { ctxAtom, runtimeAtom, wasmAtom } from '../../../atoms';
import { useAtomCallback } from 'jotai/utils'
import { vscode } from '../../../vscode'
import { useCallback } from 'react'
import {
  type TestState,
  testCaseAtom,
  areTestsRunningAtom,
  selectedTestcaseAtom,
  selectedFunctionAtom,
} from '../../atoms'
import { isParallelTestsEnabledAtom, testHistoryAtom, selectedHistoryIndexAtom, type TestHistoryRun } from './atoms'
import { isClientCallGraphEnabledAtom } from '../../preview-toolbar'
import { apiKeysAtom } from '../../../../../components/api-keys-dialog/atoms';

// Helper function to clear highlights if in VSCode
const clearHighlights = () => {
  try {
    vscode.postMessage({
      command: 'clearHighlights',
    })
  } catch (e) {
    console.error('Failed to clear highlights in VSCode:', e)
  }
}

// TODO: use a single hook for both run and parallel run
const useRunTests = (maxBatchSize = 5) => {
  const { rt } = useAtomValue(runtimeAtom)
  const ctx = useAtomValue(ctxAtom)
  const wasm = useAtomValue(wasmAtom)
  const setSelectedTestcase = useSetAtom(selectedTestcaseAtom)
  const setSelectedFunction = useSetAtom(selectedFunctionAtom)
  const setIsClientCallGraphEnabled = useSetAtom(isClientCallGraphEnabledAtom)
  const apiKeys = useAtomValue(apiKeysAtom)
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
          console.log('runTest', test)
          console.log('apiKeys', apiKeys)

          // TEMPORARY DEBUGGING HELPER:
          // console.log("Try to set flashing regions")
          // try {
          //   vscode.postMessage({
          //     command: 'set_flashing_regions',
          //     spans: [{file_path: "tmp", start: 1, end: 4, start_line:0, end_line: 0}],
          //   })
          // } catch (e) {
          //   console.error('Failed to set flashing regions in VSCode:', e)
          // }

          vscode.postMessage({
            command: 'telemetry',
            meta: {
              action: 'run_tests',
              data: {
                num_tests: tests.length,
                parallel: false,
              },
            },
          })

          try {
            const testCase = get(testCaseAtom(test))
            if (!rt || !ctx || !testCase || !wasm) {
              setState(test, {
                status: 'error',
                message: 'Missing required dependencies. Try reloading the playground.',
              })
              console.error('Missing required dependencies. Try reloading the playground.')
              clearHighlights() // Clear highlights on error
              return
            }

            const startTime = performance.now()
            setState(test, { status: 'running' })

            const result = await testCase.fn.run_test_with_expr_events(
              rt,
              testCase.tc.name,
              (partial: WasmFunctionResponse) => {
                setState(test, { status: 'running', response: partial })
              },
              findMediaFile,
              (spans: WasmSpan[]) => {
                // Send spans to VSCode for highlighting if we're in the VSCode environment
                const spans_to_send = spans.map((span) => ({
                  file_path: span.file_path,
                  start_line: span.start_line,
                  start: span.start,
                  end_line: span.end_line,
                  end: span.end,
                }))
                console.log('spans_to_send: ', spans_to_send)
                try {
                  vscode.postMessage({
                    command: 'set_flashing_regions',
                    content: { spans: spans_to_send },
                  })
                } catch (e) {
                  console.error('Failed to send spans to VSCode:', e)
                }
              },
              // TODO this needs to be moved down cause its wrong param.
              apiKeys,
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

            // Clear highlights when test is completed, whether success or failure
            clearHighlights()
          } catch (e) {
            console.log('test error!')
            console.error(e)
            clearHighlights() // Clear highlights on error
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

          if (tests.length === 0 || !tests[0]) {
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
          clearHighlights() // Clear highlights when all tests are done
        })
      },
      [maxBatchSize, rt, ctx, wasm, apiKeys],
    ),
  )

  return { setRunningTests: runTests }
}

const useParallelRunTests = (maxBatchSize = 5) => {
  const { rt } = useAtomValue(runtimeAtom)
  const ctx = useAtomValue(ctxAtom)
  const wasm = useAtomValue(wasmAtom)
  const setSelectedTestcase = useSetAtom(selectedTestcaseAtom)
  const setSelectedFunction = useSetAtom(selectedFunctionAtom)
  const setIsClientCallGraphEnabled = useSetAtom(isClientCallGraphEnabledAtom)
  const apiKeys = useAtomValue(apiKeysAtom)
  const runParallelTests = useAtomCallback(
    useCallback(
      async (get, set, tests: { functionName: string; testName: string }[]) => {
        // if tests are already running just return
        if (get(areTestsRunningAtom)) {
          return
        }

        // Create a new history run
        const historyRun: TestHistoryRun = {
          timestamp: Date.now(),
          tests: tests.map((test) => ({
            timestamp: Date.now(),
            functionName: test.functionName,
            testName: test.testName,
            response: { status: 'running' },
            input: get(testCaseAtom(test))?.tc.inputs,
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

        const run = async () => {
          if (!rt || !ctx || !wasm) {
            console.error('Missing required dependencies')
            return
          }

          if (tests.length === 0) {
            console.error('No tests found')
            return
          }

          if (tests.length === 0 || !tests[0]) {
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

          vscode.postMessage({
            command: 'telemetry',
            meta: {
              action: 'run_tests',
              data: {
                num_tests: tests.length,
                parallel: true,
              },
            },
          })

          try {
            // Prepare test cases for `run_tests`
            const testCases = tests
              .map((test) => {
                const testCase = get(testCaseAtom(test))
                if (!testCase) {
                  setState(test, { status: 'error', message: 'Test case not found' })
                  return null
                }
                return {
                  functionName: testCase.fn.name,
                  testName: testCase.tc.name,
                  inputs: testCase.tc.inputs,
                }
              })
              .filter(Boolean)

            if (testCases.length === 0) {
              console.error('No valid test cases found')
              return
            }

            const startTime = performance.now()
            set(areTestsRunningAtom, true)

            // Call `run_tests` on the runtime
            const results = await rt.run_tests(
              testCases,
              (partial: WasmFunctionResponse) => {
                const pair = partial.func_test_pair()
                setState(
                  { functionName: pair.function_name, testName: pair.test_name },
                  { status: 'running', response: partial },
                )
              },
              findMediaFile,
              apiKeys,
            )

            const endTime = performance.now()
            const responseStatusMap = {
              [wasm.TestStatus.Passed]: 'passed',
              [wasm.TestStatus.LLMFailure]: 'llm_failed',
              [wasm.TestStatus.ParseFailure]: 'parse_failed',
              [wasm.TestStatus.ConstraintsFailed]: 'constraints_failed',
              [wasm.TestStatus.AssertFailed]: 'assert_failed',
              [wasm.TestStatus.UnableToRun]: 'error',
              [wasm.TestStatus.FinishReasonFailed]: 'error',
            } as const

            // Process results
            // TODO: is there a better way to handle Rust's Option? Or do we even need Option?
            let response: WasmTestResponse | null | undefined
            while ((response = results.yield_next()) != undefined) {
              const pair = response.func_test_pair()
              const status = response.status()
              setState(
                { functionName: pair.function_name, testName: pair.test_name },
                {
                  status: 'done',
                  response: response,
                  response_status: responseStatusMap[status] || 'error',
                  latency_ms: endTime - startTime,
                },
              )
            }
          } catch (e) {
            console.error('Error running tests:', e)
            tests.forEach((test) => {
              setState(test, {
                status: 'error',
                message: e instanceof Error ? e.message : 'Unknown error',
              })
            })
          } finally {
            set(areTestsRunningAtom, false)
          }
        }

        await run()
      },
      [maxBatchSize, rt, ctx, wasm, apiKeys],
    ),
  )

  return { setParallelTests: runParallelTests }
}

export const useRunBamlTests = () => {
  const { setRunningTests } = useRunTests()
  const { setParallelTests } = useParallelRunTests()
  const isParallelTestsEnabled = useAtomValue(isParallelTestsEnabledAtom)

  const runTests = (tests: { functionName: string; testName: string }[]) => {
    if (isParallelTestsEnabled) {
      setParallelTests(tests)
    } else {
      setRunningTests(tests)
    }
  }

  return runTests
}
