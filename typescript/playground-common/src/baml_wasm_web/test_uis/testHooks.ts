import { atom, useAtomValue } from 'jotai'
import { atomFamily, useAtomCallback } from 'jotai/utils'
import React, { useCallback } from 'react'
import { selectedFunctionAtom, selectedRuntimeAtom } from '../EventListener'
import type { WasmFunctionResponse, WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'

const isRunningAtom = atom(false)
export const showTestsAtom = atom(false)

export type TestStatusType = 'queued' | 'running' | 'done' | 'error'
export type DoneTestStatusType = 'passed' | 'llm_failed' | 'parse_failed' | 'error'
export type TestState =
  | {
      status: 'queued'
    }
  | {
      status: 'running'
      response?: WasmFunctionResponse
    }
  | {
      status: 'done'
      response_status: DoneTestStatusType
      response: WasmTestResponse
      latency_ms: number
    }
  | {
      status: 'error'
      message: string
    }

// const statusAtom = atom<TestState>({ status: 'queued' })

export const testStatusAtom = atomFamily(
  (testName: string) => atom<TestState>({ status: 'queued' }),
  (a, b) => a === b,
)
export const runningTestsAtom = atom<string[]>([])
export const statusCountAtom = atom({
  queued: 0,
  running: 0,
  done: {
    passed: 0,
    llm_failed: 0,
    parse_failed: 0,
    error: 0,
  },
  error: 0,
})

export const useRunHooks = () => {
  const isRunning = useAtomValue(isRunningAtom)

  const runTest = useAtomCallback(
    useCallback(
      async (get, set, testNames: string[]) => {
        const runtime = get(selectedRuntimeAtom)
        const func = get(selectedFunctionAtom)
        if (!runtime || !func) {
          // Refuse to run a test if no runtime is selected
          return
        }
        const isRunning = get(isRunningAtom)
        if (isRunning) {
          // Refuse to run another test if one is already running
          return
        }
        set(isRunningAtom, true)
        set(showTestsAtom, true)

        // First clear any previous test results
        testStatusAtom.setShouldRemove(() => true)
        // Remove the shouldRemove function so we don't remove future test results
        testStatusAtom.setShouldRemove(null)

        set(runningTestsAtom, testNames)
        set(statusCountAtom, {
          queued: testNames.length,
          running: 0,
          done: {
            passed: 0,
            llm_failed: 0,
            parse_failed: 0,
            error: 0,
          },
          error: 0,
        })

        // TODO: @hellovai find out why large batch sizes don't work
        const batchSize = 1
        const batches = []
        for (let i = 0; i < testNames.length; i += batchSize) {
          batches.push(testNames.slice(i, i + batchSize))
        }
        for (const batch of batches) {
          const promises = await Promise.allSettled(
            batch.map(async (testName) => {
              set(testStatusAtom(testName), { status: 'running' })
              set(statusCountAtom, (prev) => {
                return {
                  ...prev,
                  running: prev.running + 1,
                  queued: prev.queued - 1,
                }
              })
              if (!func || !runtime) {
                return Promise.reject(new Error('Code potentially modified while running tests'))
              }
              let now = new Date().getTime()
              return func
                .run_test(runtime, testName, (intermediate: WasmFunctionResponse) => {
                  set(testStatusAtom(testName), {
                    status: 'running',
                    response: intermediate,
                  })
                })
                .then((res) => {
                  let elapsed = new Date().getTime() - now
                  return { res, elapsed }
                })
            }),
          )
          for (let i = 0; i < promises.length; i++) {
            const result = promises[i]
            if (result.status === 'fulfilled') {
              const { res, elapsed } = result.value
              // console.log('result', i, result.value.res.llm_response(), 'batch[i]', batch[i])

              let status = res.status()
              let response_status: DoneTestStatusType = 'error'
              if (status === 0) {
                response_status = 'passed'
              } else if (status === 1) {
                response_status = 'llm_failed'
              } else if (status === 2) {
                response_status = 'parse_failed'
              } else {
                response_status = 'error'
              }
              set(testStatusAtom(batch[i]), {
                status: 'done',
                response_status,
                response: res,
                latency_ms: elapsed,
              })
              set(statusCountAtom, (prev) => {
                return {
                  ...prev,
                  done: {
                    ...prev.done,
                    [response_status]: prev.done[response_status] + 1,
                  },
                  running: prev.running - 1,
                }
              })
            } else {
              set(testStatusAtom(batch[i]), { status: 'error', message: `${result.reason}` })
              set(statusCountAtom, (prev) => {
                return {
                  ...prev,
                  error: prev.error + 1,
                  running: prev.running - 1,
                }
              })
            }
          }
        }

        set(isRunningAtom, false)
      },
      [isRunningAtom, selectedRuntimeAtom, selectedFunctionAtom],
    ),
  )

  return { isRunning, run: runTest }
}
