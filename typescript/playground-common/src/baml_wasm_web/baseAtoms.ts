import type { WasmDiagnosticError, WasmProject, WasmRuntime } from '@gloo-ai/baml-schema-wasm-web'
import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'

export const availableProjectsAtom = atom<string[]>([])

const filesAtom = atom<Record<string, string>>({})
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
