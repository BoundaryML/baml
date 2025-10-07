import type { WasmFunctionResponse, WasmSpan, WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web'
import { useAtomValue, useSetAtom } from 'jotai'
import { ctxAtom, runtimeAtom, wasmAtom, wasmPanicAtom } from '../../../atoms';
import { useAtomCallback } from 'jotai/utils'
import { vscode } from '../../../vscode'
import { useCallback, useEffect } from 'react'
import {
  type TestState,
  testCaseAtom,
  areTestsRunningAtom,
  selectedTestcaseAtom,
  selectedFunctionAtom,
  currentAbortControllerAtom,
  flashRangesAtom,
} from '../../atoms'
import { isParallelTestsEnabledAtom, testHistoryAtom, selectedHistoryIndexAtom, type TestHistoryRun } from './atoms'
import { isClientCallGraphEnabledAtom } from '../../preview-toolbar'
import { apiKeysAtom } from '../../../../../components/api-keys-dialog/atoms';


// TODO: use a single hook for both run and parallel run
const useRunTests = (maxBatchSize = 5) => {
  const { rt } = useAtomValue(runtimeAtom)
  const ctx = useAtomValue(ctxAtom)
  const wasm = useAtomValue(wasmAtom)
  const setSelectedTestcase = useSetAtom(selectedTestcaseAtom)
  const setSelectedFunction = useSetAtom(selectedFunctionAtom)
  const setIsClientCallGraphEnabled = useSetAtom(isClientCallGraphEnabledAtom)
  const apiKeys = useAtomValue(apiKeysAtom)
  const setFlashRanges = useSetAtom(flashRangesAtom)
  const runTests = useAtomCallback(
    useCallback(
      async (get, set, tests: { functionName: string; testName: string }[]) => {
        // Create a fresh abort controller for this test run
        const controller = new AbortController()
        set(currentAbortControllerAtom, controller)

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

          vscode.sendTelemetry({
            action: 'run_tests',
            data: {
              num_tests: tests.length,
              parallel: false,
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
              return
            }

            const startTime = performance.now()
            setState(test, { status: 'running' })

            console.warn('BAML Cancel: Passing abort signal to run_test_with_expr_events', {
              testName: testCase.tc.name,
              hasSignal: !!controller.signal,
              signalAborted: controller.signal.aborted
            })
            console.log('[TestRunner] Starting run_test_with_expr_events', {
              functionName: testCase.fn.name,
              testCaseName: testCase.tc.name,
              signature: testCase.fn.signature,
              abortSignalAborted: controller.signal.aborted,
            })
            const result = await testCase.fn.run_test_with_expr_events(
              rt,
              testCase.tc.name,
              (partial: WasmFunctionResponse) => {
                setState(test, { status: 'running', response: partial })
              },
              vscode.loadMediaFile,
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
                  vscode.setFlashingRegions(spans_to_send)
                  setFlashRanges(spans_to_send.map((span) => ({
                    filePath: span.file_path,
                    startLine: span.start_line,
                    startCol: span.start,
                    endLine: span.end_line,
                    endCol: span.end,
                  })))
                } catch (e) {
                  console.error('Failed to send spans to VSCode:', e)
                }
              },
              // TODO this needs to be moved down cause its wrong param.
              apiKeys,
              controller.signal, // Pass abort signal
            )
            console.log('result', result)

            const endTime = performance.now()
            const response_status = result.status()
            console.log('[TestRunner] run_test_with_expr_events completed', {
              functionName: testCase.fn.name,
              testCaseName: testCase.tc.name,
              responseStatus: response_status,
            })
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
            console.error('[TestRunner] run_test_with_expr_events error', {
              functionName: test.functionName,
              testCaseName: test.testName,
              error: e,
            })

            // Check if this is an abort error
            if (e instanceof Error && (e.name === 'AbortError' || e.message?.includes('BamlAbortError'))) {
              setState(test, {
                status: 'error',
                message: 'Test execution was cancelled by user',
              })
            } else {
              setState(test, {
                status: 'error',
                message: e instanceof Error ? e.message : 'Unknown error',
              })
            }
          }
        }

        const run = async () => {
          console.warn('BAML Cancel: run() function started, tests:', tests)
          // Create batches of tests to run
          const batches: { functionName: string; testName: string }[][] = []
          for (let i = 0; i < tests.length; i += maxBatchSize) {
            batches.push(tests.slice(i, i + maxBatchSize))
          }
          console.warn('BAML Cancel: Created batches:', batches)

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
          console.warn('BAML Cancel: Starting to run batches, batch count:', batches.length)
          for (const batch of batches) {
            console.warn('BAML Cancel: Processing batch with tests:', batch)
            // TODO: parallelize when we fix wasm issues with runtime undefined after multiple runs
            for (const test of batch) {
              console.warn('BAML Cancel: About to run test:', test)
              setState(test, { status: 'queued' })
              await runTest(test)
              console.warn('BAML Cancel: Finished running test:', test)
            }
          }
        }

        console.warn('BAML Cancel: About to set areTestsRunningAtom to true and call run()')
        set(areTestsRunningAtom, true)
        console.warn('BAML Cancel: Calling run() now')
        await run().finally(() => {
          console.warn('BAML Cancel: Tests completed, cleaning up')
          set(areTestsRunningAtom, false)
          set(currentAbortControllerAtom, null) // Clean up abort controller
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

        // Create a fresh abort controller for this test run
        const controller = new AbortController()
        console.warn('BAML Cancel: Created new AbortController for test run')
        set(currentAbortControllerAtom, controller)
        console.warn('BAML Cancel: AbortController stored in atom')

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

          vscode.sendTelemetry({
            action: 'run_tests',
            data: {
              num_tests: tests.length,
              parallel: true,
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
              vscode.loadMediaFile,
              apiKeys,
              controller.signal, // Now supported!
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

              // Debug: Log the response details
              console.log('[DEBUG] Test response received:', {
                functionName: pair.function_name,
                testName: pair.test_name,
                status: status,
                hasParsedResponse: !!response.parsed_response(),
                parsedResponse: response.parsed_response(),
                hasLlmResponse: !!response.llm_response(),
                failureMessage: response.failure_message(),
              })

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
            set(currentAbortControllerAtom, null) // Clean up abort controller
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
  const currentAbortController = useAtomValue(currentAbortControllerAtom)
  const setCurrentAbortController = useSetAtom(currentAbortControllerAtom)
  const setAreTestsRunning = useSetAtom(areTestsRunningAtom)
  const panicState = useAtomValue(wasmPanicAtom)
  const setWasmPanic = useSetAtom(wasmPanicAtom)
  const setTestHistory = useSetAtom(testHistoryAtom)

  // Automatically cancel tests when WASM panics
  useEffect(() => {
    if (panicState && currentAbortController) {
      console.error('[WASM Panic] Detected panic during test run, cancelling tests:', panicState.msg)

      // Send telemetry about the panic
      vscode.sendTelemetry({
        action: 'wasm_panic',
        data: {
          panic_message: panicState.msg,
          timestamp: panicState.timestamp,
          during_test_execution: true,
        },
      })

      // Abort the controller
      currentAbortController.abort()
      setCurrentAbortController(null)
      setAreTestsRunning(false)

      // Mark all running tests as cancelled due to panic
      setTestHistory((prev) => {
        const newHistory = [...prev]
        const currentRun = newHistory[0]
        if (!currentRun) return prev

        // Update all tests that are still running to show they were cancelled
        currentRun.tests = currentRun.tests.map((test) => {
          if (test.response.status === 'running' || test.response.status === 'queued') {
            return {
              ...test,
              response: {
                status: 'error',
                message: `WASM panic: ${panicState.msg}`,
              },
              timestamp: Date.now(),
            }
          }
          return test
        })

        return newHistory
      })
    }
  }, [panicState, currentAbortController, setCurrentAbortController, setAreTestsRunning, setTestHistory])

  const runTests = (tests: { functionName: string; testName: string }[]) => {
    console.warn('BAML Cancel: runTests called with', tests.length, 'tests, parallel:', isParallelTestsEnabled)

    // Clear any previous panic state before starting new tests
    if (panicState) {
      console.log('[WASM Panic] Clearing previous panic state before starting new tests')
      setWasmPanic(null)
    }

    if (isParallelTestsEnabled) {
      console.warn('BAML Cancel: Calling setParallelTests')
      setParallelTests(tests)
    } else {
      console.warn('BAML Cancel: Calling setRunningTests')
      setRunningTests(tests)
    }
    console.warn('BAML Cancel: runTests finished calling set function')
  }

  const cancelTests = useCallback(() => {
    console.warn('BAML Cancel: cancelTests called')
    // Abort the current controller if it exists
    if (currentAbortController) {
      console.warn('BAML Cancel: Found active abort controller, calling abort()')
      currentAbortController.abort()
      console.warn('BAML Cancel: abort() called, clearing controller')
      setCurrentAbortController(null)
      setAreTestsRunning(false)
    } else {
      console.warn('BAML Cancel: No active abort controller found')
    }
  }, [currentAbortController, setCurrentAbortController, setAreTestsRunning])

  return { runTests, cancelTests }
}
