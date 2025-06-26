// import { selectedFunctionAtom, selectedRuntimeAtom } from '../EventListener'
import type { WasmFunctionResponse, WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import { atom, useAtomValue } from 'jotai'
import { atomFamily, useAtomCallback } from 'jotai/utils'
import React, { useCallback, useEffect, useState } from 'react'
// import { vscode } from '../../../../../../playground-common/src/shared/baml-project-panel/vscode'

// const isRunningAtom = atom(false)
// export const showTestsAtom = atom(false)
// export const showClientGraphAtom = atom(false)

// export type TestStatusType = 'queued' | 'running' | 'done' | 'error'
// export type DoneTestStatusType =
//   | 'passed'
//   | 'llm_failed'
//   | 'finish_reason_failed'
//   | 'parse_failed'
//   | 'constraints_failed'
//   | 'error'
// export type TestState =
//   | {
//       status: 'queued'
//     }
//   | {
//       status: 'running'
//       response?: WasmFunctionResponse
//     }
//   | {
//       status: 'done'
//       response_status: DoneTestStatusType
//       response: WasmTestResponse
//       latency_ms: number
//     }
//   | {
//       status: 'error'
//       message: string
//     }

// // const statusAtom = atom<TestState>({ status: 'queued' })

// export const testStatusAtom = atomFamily(
//   (testName: string) => atom<TestState>({ status: 'queued' }),
//   (a, b) => a === b,
// )
// export const runningTestsAtom = atom<string[]>([])

// // Match the Rust enum
// // engine/baml-schema-wasm/src/runtime_wasm/mod.rs
// enum RustTestStatus {
//   Passed,
//   LLMFailure,
//   ParseFailure,
//   FinishReasonFailed,
//   ConstraintsFailed,
//   AssertFailed,
//   UnableToRun,
// }

// export const statusCountAtom = atom({
//   queued: 0,
//   running: 0,
//   done: {
//     passed: 0,
//     llm_failed: 0,
//     finish_reason_failed: 0,
//     parse_failed: 0,
//     constraints_failed: 0,
//     error: 0,
//   },
//   error: 0,
// })

// /// This atom will track the state of the full test suite.
// /// 'unknown` means tests haven't been run yet. `pass` means
// /// they have all run to completion.
// /// 'warn' means at least one check has failed, and `fail`
// /// means at least one assert has failed, or an internal error
// /// occurred.
// export type TestSuiteSummary = 'pass' | 'warn' | 'fail' | 'unknown'
// export const testSuiteSummaryAtom = atom<TestSuiteSummary>('unknown')

// /// For an old summary and a new result, compute the new summary.
// /// The new summary will overwrite the old, unless the old one
// /// has higher priority.
// function updateTestSuiteState(old_result: TestSuiteSummary, new_result: TestSuiteSummary): TestSuiteSummary {
//   function priority(x: TestSuiteSummary): number {
//     switch (x) {
//       case 'unknown':
//         return 0
//       case 'pass':
//         return 1
//       case 'warn':
//         return 2
//       case 'fail':
//         return 3
//     }
//   }

//   if (priority(new_result) > priority(old_result)) {
//     return new_result
//   } else {
//     return old_result
//   }
// }

// export const useRunHooks = () => {
//   const isRunning = useAtomValue(isRunningAtom)

//   const [enableProxy, setEnableProxy] = useState<undefined | boolean>()
//   useEffect(() => {
//     ;(async () => {
//       const res = await vscode.getIsProxyEnabled()
//       console.log('enableproxy call')
//       setEnableProxy(res)
//     })()
//   }, [])

//   const runTest = useAtomCallback(
//     useCallback(
//       async (get, set, testNames: string[]) => {
//         const runtime = get(selectedRuntimeAtom)
//         const func = get(selectedFunctionAtom)
//         if (!runtime || !func) {
//           // Refuse to run a test if no runtime is selected
//           return
//         }
//         const isRunning = get(isRunningAtom)
//         if (isRunning) {
//           // Refuse to run another test if one is already running
//           return
//         }
//         set(isRunningAtom, true)
//         set(showTestsAtom, true)
//         set(testSuiteSummaryAtom, 'unknown')

//         vscode.postMessage({
//           command: 'telemetry',
//           meta: {
//             action: 'run_tests',
//             data: {
//               num_tests: testNames.length,
//             },
//           },
//         })

//         // First clear any previous test results
//         testStatusAtom.setShouldRemove(() => true)
//         // Remove the shouldRemove function so we don't remove future test results
//         testStatusAtom.setShouldRemove(null)

//         set(runningTestsAtom, testNames)
//         set(statusCountAtom, {
//           queued: testNames.length,
//           running: 0,
//           done: {
//             passed: 0,
//             llm_failed: 0,
//             finish_reason_failed: 0,
//             parse_failed: 0,
//             constraints_failed: 0,
//             error: 0,
//           },
//           error: 0,
//         })

//         // TODO: @hellovai find out why large batch sizes don't work
//         const batchSize = 1
//         const batches = []
//         for (let i = 0; i < testNames.length; i += batchSize) {
//           batches.push(testNames.slice(i, i + batchSize))
//         }
//         for (const batch of batches) {
//           const promises = await Promise.allSettled(
//             batch.map(async (testName) => {
//               set(testStatusAtom(testName), { status: 'running' })
//               set(statusCountAtom, (prev) => {
//                 return {
//                   ...prev,
//                   running: prev.running + 1,
//                   queued: prev.queued - 1,
//                 }
//               })
//               if (!func || !runtime) {
//                 return Promise.reject(new Error('Code potentially modified while running tests'))
//               }
//               let now = new Date().getTime()
//               return func
//                 .run_test(
//                   runtime,
//                   testName,
//                   (intermediate: WasmFunctionResponse) => {
//                     set(testStatusAtom(testName), {
//                       status: 'running',
//                       response: intermediate,
//                     })
//                   },
//                   async (path: string) => {
//                     return await vscode.readFile(path)
//                   },
//                 )
//                 .then((res) => {
//                   let elapsed = new Date().getTime() - now
//                   return { res, elapsed }
//                 })
//             }),
//           )
//           for (let i = 0; i < promises.length; i++) {
//             const result = promises[i]
//             if (result.status === 'fulfilled') {
//               const { res, elapsed } = result.value
//               // console.log('result', i, result.value.res.llm_response(), 'batch[i]', batch[i])

//               let status: RustTestStatus = res.status() as unknown as RustTestStatus
//               let response_status: DoneTestStatusType = 'error'
//               if (status === RustTestStatus.Passed) {
//                 response_status = 'passed'
//               } else if (status === RustTestStatus.LLMFailure) {
//                 response_status = 'llm_failed'
//               } else if (status === RustTestStatus.ParseFailure) {
//                 response_status = 'parse_failed'
//               } else if (status === RustTestStatus.FinishReasonFailed) {
//                 response_status = 'finish_reason_failed'
//               } else if (status === RustTestStatus.ConstraintsFailed || status === RustTestStatus.AssertFailed) {
//                 response_status = 'constraints_failed'
//               } else {
//                 response_status = 'error'
//               }
//               set(testStatusAtom(batch[i]), {
//                 status: 'done',
//                 response_status,
//                 response: res,
//                 latency_ms: elapsed,
//               })
//               set(statusCountAtom, (prev) => {
//                 return {
//                   ...prev,
//                   done: {
//                     ...prev.done,
//                     [response_status]: prev.done[response_status] + 1,
//                   },
//                   running: prev.running - 1,
//                 }
//               })

//               let newTestSuiteStatus: TestSuiteSummary = 'unknown'
//               if (status === 0) {
//                 newTestSuiteStatus = 'pass'
//               } else if (status === 1) {
//                 newTestSuiteStatus = 'fail'
//               } else if (status === 2) {
//                 newTestSuiteStatus = 'fail'
//               } else if (status === 3) {
//                 newTestSuiteStatus = 'warn'
//               } else if (status === 4) {
//                 newTestSuiteStatus = 'fail'
//               }

//               let currentSummary = get(testSuiteSummaryAtom)
//               let updatedSummary = updateTestSuiteState(currentSummary, newTestSuiteStatus)
//               set(testSuiteSummaryAtom, updatedSummary)
//             } else {
//               set(testStatusAtom(batch[i]), { status: 'error', message: `${result.reason}` })
//               set(statusCountAtom, (prev) => {
//                 return {
//                   ...prev,
//                   error: prev.error + 1,
//                   running: prev.running - 1,
//                 }
//               })
//             }
//           }
//         }

//         set(isRunningAtom, false)
//       },
//       [isRunningAtom, selectedRuntimeAtom, selectedFunctionAtom, enableProxy],
//     ),
//   )

//   return { isRunning, run: runTest }
// }
export {}
