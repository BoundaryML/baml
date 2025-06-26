import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import { type TestState } from '../../atoms'

export interface TestHistoryEntry {
  timestamp: number
  functionName: string
  testName: string
  response: TestState
  input?: any
}

export interface TestHistoryRun {
  timestamp: number
  tests: TestHistoryEntry[]
}

// TODO: make this persistent, but make sure to serialize the wasm objects properly
export const testHistoryAtom = atom<TestHistoryRun[]>([])
export const selectedHistoryIndexAtom = atom<number>(0)
export const isParallelTestsEnabledAtom = atomWithStorage<boolean>('runTestsInParallel', false)
