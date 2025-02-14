import { atom } from 'jotai'
import { requiredEnvVarsAtom, envVarsAtom, runtimeAtom } from '../atoms'

export const runtimeStateAtom = atom((get) => {
  const { rt } = get(runtimeAtom)
  console.log('rt', rt)
  if (rt === undefined) {
    return { functions: [] }
  }
  const functions = rt.list_functions()

  return { functions }
})

export const selectedFunctionAtom = atom<string | undefined>(undefined)
export const selectedTestcaseAtom = atom<string | undefined>(undefined)

export const selectedItemAtom = atom(
  (get) => {
    const selected = get(selectionAtom)
    if (selected.selectedFn === undefined || selected.selectedTc === undefined) {
      return undefined
    }
    return [selected.selectedFn.name, selected.selectedTc.name] as [string, string]
  },
  (_, set, functionName: string, testcaseName: string) => {
    set(selectedFunctionAtom, functionName)
    set(selectedTestcaseAtom, testcaseName)
  },
)

export const functionObjectAtom = atomFamily((functionName: string) =>
  atom((get) => {
    const { functions } = get(runtimeStateAtom)
    const fn = functions.find((f) => f.name === functionName)
    if (!fn) {
      return undefined
    }
    return fn
  }),
)

export const testcaseObjectAtom = atomFamily((params: { functionName: string; testcaseName: string }) =>
  atom((get) => {
    const { functions } = get(runtimeStateAtom)
    const fn = functions.find((f) => f.name === params.functionName)
    if (!fn) {
      return undefined
    }
    const tc = fn.test_cases.find((tc) => tc.name === params.testcaseName)
    if (!tc) {
      return undefined
    }
    return tc
  }),
)

export const updateCursorAtom = atom(
  null,
  (get, set, cursor: { fileName: string; fileText: string; line: number; column: number }) => {
    const runtime = get(runtimeAtom).rt

    if (runtime) {
      const fileName = cursor.fileName
      const fileContent = cursor.fileText
      const lines = fileContent.split('\n')

      let cursorIdx = 0
      for (let i = 0; i < cursor.line - 1; i++) {
        cursorIdx += lines[i].length + 1 // +1 for the newline character
      }

      cursorIdx += cursor.column

      const selectedFunc = runtime.get_function_at_position(fileName, get(selectedFunctionAtom) ?? '', cursorIdx)

      if (selectedFunc) {
        set(selectedFunctionAtom, selectedFunc.name)
        const selectedTestcase = runtime.get_testcase_from_position(selectedFunc, cursorIdx)

        if (selectedTestcase) {
          set(selectedTestcaseAtom, selectedTestcase.name)
          const nestedFunc = runtime.get_function_of_testcase(fileName, cursorIdx)

          if (nestedFunc) {
            set(selectedFunctionAtom, nestedFunc.name)
          }
        }
      }
    }
  },
)

export const selectionAtom = atom((get) => {
  const selectedFunction = get(selectedFunctionAtom)
  const selectedTestcase = get(selectedTestcaseAtom)

  const state = get(runtimeStateAtom)

  let selectedFn = state.functions.at(0)
  if (selectedFunction !== undefined) {
    const foundFn = state.functions.find((f) => f.name === selectedFunction)
    if (foundFn) {
      selectedFn = foundFn
    } else {
      console.error('Function not found', selectedFunction)
    }
  } else {
    console.log('No function selected')
  }

  let selectedTc = selectedFn?.test_cases.at(0)
  if (selectedTestcase !== undefined) {
    const foundTc = selectedFn?.test_cases.find((tc) => tc.name === selectedTestcase)
    if (foundTc) {
      selectedTc = foundTc
    } else {
      console.error('Testcase not found', selectedTestcase)
    }
  }

  return { selectedFn, selectedTc }
})

export const selectedFunctionObjectAtom = atom((get) => {
  const { selectedFn } = get(selectionAtom)
  return selectedFn
})

const hasShownEnvDialogAtom = atomWithStorage('has-closed-env-vars-dialog', false, vscodeLocalStorageStore)

const envDialogOpenAtom = atom(false)

export const showEnvDialogAtom = atom(
  (get) => {
    const envDialogOpen = get(envDialogOpenAtom)
    if (envDialogOpen) return true

    const requiredVars = get(requiredEnvVarsAtom)
    const envVars = get(envVarsAtom)

    // Check if ALL required vars are missing
    const hasMissingVars = requiredVars.length > 0 && requiredVars.every((key) => !envVars[key])

    const hasShownDialog = get(hasShownEnvDialogAtom)
    if (hasShownDialog) return envDialogOpen

    // if we are in vscode, we don't want to show the dialog
    if (!vscode.isVscode()) {
      return false
    }

    return hasMissingVars
  },
  (get, set, value: boolean) => {
    if (!value) {
      set(hasShownEnvDialogAtom, true)
    }
    set(envDialogOpenAtom, value)
  },
)

export const areEnvVarsMissingAtom = atom((get) => {
  const requiredVars = get(requiredEnvVarsAtom)
  const isVscode = vscode.isVscode()
  if (!isVscode) return false
  const envVars = get(envVarsAtom)
  return requiredVars.length > 0 && requiredVars.every((key) => !envVars[key])
})

// Related to test status
import { type WasmFunctionResponse, type WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web'
import { atomFamily, atomWithStorage } from 'jotai/utils'
import { vscodeLocalStorageStore } from '../Jotai'
import { vscode } from '../vscode'

export type TestStatusType = 'queued' | 'running' | 'done' | 'error' | 'idle'
export type DoneTestStatusType =
  | 'passed'
  | 'llm_failed'
  | 'parse_failed'
  | 'constraints_failed'
  | 'assert_failed'
  | 'error'
export type TestState =
  | {
      status: 'queued' | 'idle'
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

export const testCaseAtom = atomFamily((params: { functionName: string; testName: string }) =>
  atom((get) => {
    const { functions } = get(runtimeStateAtom)
    const fn = functions.find((f) => f.name === params.functionName)
    const tc = fn?.test_cases.find((tc) => tc.name === params.testName)
    if (!fn || !tc) {
      return undefined
    }
    return { fn, tc }
  }),
)

export const functionTestSnippetAtom = atomFamily((functionName: string) =>
  atom((get) => {
    const { functions } = get(runtimeStateAtom)
    const fn = functions.find((f) => f.name === functionName)
    if (!fn) {
      return undefined
    }
    return fn.test_snippet
  }),
)

export const testCaseResponseAtom = atomFamily((params: { functionName: string; testName: string }) =>
  atom((get) => {
    const allTestCaseResponse = get(runningTestsAtom)
    const testCaseResponse = allTestCaseResponse.find(
      (t) => t.functionName === params.functionName && t.testName === params.testName,
    )
    return testCaseResponse?.state
  }),
)
export const areTestsRunningAtom = atom(false)
// TODO: this is never set.
export const runningTestsAtom = atom<{ functionName: string; testName: string; state: TestState }[]>([])
