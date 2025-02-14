'use client'
import 'react18-json-view/src/style.css'
// import * as vscode from 'vscode'

import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'
import { useEffect } from 'react'
import CustomErrorBoundary from '../utils/ErrorFallback'
import { vscodeLocalStorageStore } from './JotaiProvider'
import { vscode } from '@/shared/baml-project-panel/vscode'
import { diagnosticsAtom, filesAtom, wasmAtom } from '@/shared/baml-project-panel/atoms'
import {
  selectedFunctionAtom,
  selectedTestcaseAtom,
  updateCursorAtom,
} from '@/shared/baml-project-panel/playground-panel/atoms'
import { useRunTests } from '@/shared/baml-project-panel/playground-panel/prompt-preview/test-panel/test-runner'
import { orchIndexAtom } from '@/shared/baml-project-panel/playground-panel/atoms-orch-graph'
import { CodeMirrorDiagnosticsAtom } from '@/shared/baml-project-panel/codemirror-panel/atoms'
import { AlertTriangle, XCircle } from 'lucide-react'
import { CheckCircle } from 'lucide-react'
import { useDebounce, useDebounceCallback } from '@react-hook/debounce'

export const hasClosedEnvVarsDialogAtom = atomWithStorage<boolean>(
  'has-closed-env-vars-dialog',
  false,
  vscodeLocalStorageStore,
)
export const bamlCliVersionAtom = atom<string | null>(null)

export const showIntroToChecksDialogAtom = atom(false)
export const hasClosedIntroToChecksDialogAtom = atomWithStorage<boolean>(
  'has-closed-intro-to-checks-dialog',
  false,
  vscodeLocalStorageStore,
)

export const versionAtom = atom((get) => {
  const wasm = get(wasmAtom)

  if (wasm === undefined) {
    return 'Loading...'
  }

  return wasm.version()
})

export const numErrorsAtom = atom((get) => {
  const errors = get(diagnosticsAtom)

  const warningCount = errors.filter((e: any) => e.type === 'warning').length

  return { errors: errors.length - warningCount, warnings: warningCount }
})

const ErrorCount: React.FC = () => {
  const { errors, warnings } = useAtomValue(numErrorsAtom)
  if (errors === 0 && warnings === 0) {
    return (
      <div className='flex flex-row gap-1 items-center text-green-600'>
        <CheckCircle size={12} />
      </div>
    )
  }
  if (errors === 0) {
    return (
      <div className='flex flex-row gap-1 items-center text-yellow-600'>
        {warnings} <AlertTriangle size={12} />
      </div>
    )
  }
  return (
    <div className='flex flex-row gap-1 items-center text-red-600'>
      {errors} <XCircle size={12} /> {warnings} <AlertTriangle size={12} />{' '}
    </div>
  )
}

// We don't use ASTContext.provider because we should the default value of the context
export const EventListener: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const updateCursor = useSetAtom(updateCursorAtom)
  const setFiles = useSetAtom(filesAtom)
  const debouncedSetFiles = useDebounceCallback(setFiles, 50, true)

  const [selectedFunc, setSelectedFunction] = useAtom(selectedFunctionAtom)
  const setSelectedTestcase = useSetAtom(selectedTestcaseAtom)
  const [bamlCliVersion, setBamlCliVersion] = useAtom(bamlCliVersionAtom)
  const { setRunningTests } = useRunTests()
  const wasm = useAtomValue(wasmAtom)
  useEffect(() => {
    if (wasm) {
      console.log('wasm ready!')
      try {
        vscode.markInitialized()
      } catch (e) {
        console.error('Error marking initialized', e)
      }
    }
  }, [wasm])

  const setOrchestratorIndex = useSetAtom(orchIndexAtom)

  useEffect(() => {
    if (selectedFunc) {
      // todo: maybe we use a derived atom to reset it. But for now this useeffect works.
      setOrchestratorIndex(0)
    }
  }, [selectedFunc])
  console.log('selectedFunc', selectedFunc)

  useEffect(() => {
    console.log('adding event listener')
    const fn = (
      event: MessageEvent<
        | {
            command: 'modify_file'
            content: {
              root_path: string
              name: string
              content: string | undefined
            }
          }
        | {
            command: 'add_project'
            content: {
              root_path: string
              files: Record<string, string>
            }
          }
        | {
            command: 'remove_project'
            content: {
              root_path: string
            }
          }
        | {
            command: 'select_function'
            content: {
              root_path: string
              function_name: string
            }
          }
        | {
            command: 'update_cursor'
            content: {
              cursor: { fileName: string; fileText: string; line: number; column: number }
            }
          }
        | {
            command: 'port_number'
            content: {
              port: number
            }
          }
        | {
            command: 'baml_cli_version'
            content: string
          }
        | {
            command: 'run_test'
            content: { test_name: string }
          }
      >,
    ) => {
      const { command, content } = event.data
      console.log('command', command)

      switch (command) {
        case 'add_project':
          if (content && content.root_path) {
            console.log('add_project', content.root_path)
            debouncedSetFiles(
              Object.fromEntries(Object.entries(content.files).map(([name, content]) => [name, content])),
            )
          }
          break

        case 'select_function':
          console.log('select_function', content)
          setSelectedFunction(content.function_name)
          break
        case 'update_cursor':
          if ('cursor' in content) {
            updateCursor(content.cursor)
          }
          break
        case 'baml_cli_version':
          console.log('baml_cli_version', content)
          setBamlCliVersion(content)
          break

        case 'remove_project':
          setFiles({})
          break

        case 'run_test':
          if (selectedFunc) {
            setSelectedTestcase(content.test_name)
            setRunningTests([{ functionName: selectedFunc, testName: content.test_name }])
          } else {
            console.error('No function selected')
          }
          // run([content.test_name])
          // setShowTests(true)
          // setClientGraph(false)
          break
      }
    }

    window.addEventListener('message', fn)

    return () => window.removeEventListener('message', fn)
    // If we dont add the jotai atom callbacks here like setRunningTests, this will call an old version of the atom (e.g. runTests which may have undefined dependencies).
  }, [selectedFunc, setRunningTests, updateCursor])

  const version = useAtomValue(versionAtom)

  return (
    <>
      <div className='flex absolute right-2 bottom-2 z-50 flex-row gap-2 text-xs bg-transparent'>
        <div className='pr-4 whitespace-nowrap'>{bamlCliVersion && 'baml-cli ' + bamlCliVersion}</div>
        <ErrorCount /> <span className='text-muted-foreground text-[10px]'>VSCode Runtime Version: {version}</span>
      </div>
      <CustomErrorBoundary message='Error loading project'>{children}</CustomErrorBoundary>
    </>
  )
}
