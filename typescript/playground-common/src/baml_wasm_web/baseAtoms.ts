import { atom } from 'jotai'
import { atomFamily } from 'jotai/utils'
import type { WasmProject, WasmRuntime, WasmDiagnosticError } from '@gloo-ai/baml-schema-wasm-web'

export const availableProjectsAtom = atom<string[]>([])

const filesAtom = atom<Record<string, string>>({})
const projectAtom = atom<WasmProject | null>(null)
const runtimesAtom = atom<{
  last_successful_runtime?: WasmRuntime
  current_runtime?: WasmRuntime
  diagnostics?: WasmDiagnosticError
}>({})

export const projectFamilyAtom = atomFamily((root_path: string) => projectAtom)
export const runtimeFamilyAtom = atomFamily((root_path: string) => runtimesAtom)
export const projectFilesAtom = atomFamily((root_path: string) => filesAtom)
