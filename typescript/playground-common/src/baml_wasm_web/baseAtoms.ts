import type { WasmDiagnosticError, WasmProject, WasmRuntime } from '@gloo-ai/baml-schema-wasm-web'
import { atom } from 'jotai'
import { atomFamily, atomWithStorage } from 'jotai/utils'
import { sessionStore } from './JotaiProvider'

export const availableProjectsAtom = atomWithStorage<string[]>('available_projects', [], sessionStore)
const filesAtom = atomWithStorage<Record<string, string>>('files', {}, sessionStore)

const projectAtom = atom<WasmProject | null>(null)
const runtimesAtom = atom<{
  last_attempt: 'success' | 'error' | 'no_attempt_yet'
  last_successful_runtime?: WasmRuntime
  current_runtime?: WasmRuntime
  diagnostics?: WasmDiagnosticError
}>({ last_attempt: 'no_attempt_yet' })

export const projectFamilyAtom = atomFamily((root_path: string) => projectAtom)
export const runtimeFamilyAtom = atomFamily((root_path: string) => runtimesAtom)
export const projectFilesAtom = atomFamily((root_path: string) => filesAtom)
