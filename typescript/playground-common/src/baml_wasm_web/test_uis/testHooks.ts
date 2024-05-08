import { atom, useAtomValue } from 'jotai'
import { atomFamily, useAtomCallback } from 'jotai/utils'
import React from 'react'
import { runtimeCtx, selectedFunctionAtom, selectedRuntimeAtom } from '../EventListener'
import { WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'

const isRunningAtom = atom(false)
export const showTestsAtom = atom(false)

export type TestStatusType = 'queued' | 'running' | 'done' | 'error'
export type DoneTestStatusType = 'passed' | 'llm_failed' | 'parse_failed' | 'error'
export type TestState =
  | {
      status: 'queued' | 'running'
    }
  | {
      status: 'done'
      response_status: DoneTestStatusType
      response: WasmTestResponse
    }
  | {
      status: 'error'
      message: string
    }

const statusAtom = atom<TestState>({ status: 'queued' })

export const testStatusAtom = atomFamily((testName: string) => statusAtom)
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

  const runTest = useAtomCallback(async (get, set, testNames: string[]) => {
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

    const ctx = get(runtimeCtx)
    console.log('test baml getting runtime context')
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
    // Batch into groups of 5
    const batches = []
    for (let i = 0; i < testNames.length; i += 5) {
      batches.push(testNames.slice(i, i + 5))
    }
    for (const batch of batches) {
      const promises = await Promise.allSettled(
        batch.map((testName) => {
          set(testStatusAtom(testName), { status: 'running' })
          set(statusCountAtom, (prev) => {
            return {
              ...prev,
              running: prev.running + 1,
              queued: prev.queued - 1,
            }
          })
          if (!func || !runtime || !ctx) {
            return Promise.reject(new Error('Code potentially modified while running tests'))
          }
          return func.run_test(runtime, ctx, testName)
        }),
      )
      for (let i = 0; i < promises.length; i++) {
        const result = promises[i]
        if (result.status === 'fulfilled') {
          const res = result.value
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
          set(testStatusAtom(batch[i]), { status: 'done', response_status, response: res })
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
  })

  return { isRunning, run: runTest }
}
