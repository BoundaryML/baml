import { atom, useAtomValue, useSetAtom } from 'jotai'
import { ctxAtom, diagnosticsAtom, runtimeAtom } from '../../atoms'
import { areTestsRunningAtom, functionTestSnippetAtom, selectionAtom } from '../atoms'
import type { WasmPrompt, WasmError } from '@gloo-ai/baml-schema-wasm-web'
import { Loader } from './components'
import { ErrorMessage } from './components'
import { findMediaFile } from './media-utils'
import { RenderPrompt } from './render-prompt'
import useSWR from 'swr'
import { useState } from 'react'
import { useCallback } from 'react'

export const renderedPromptAtom = atom<WasmPrompt | undefined>(undefined)

export const PromptPreviewContent = () => {
  const { rt } = useAtomValue(runtimeAtom)
  const ctx = useAtomValue(ctxAtom)
  const { selectedFn, selectedTc } = useAtomValue(selectionAtom)
  const diagnostics = useAtomValue(diagnosticsAtom)
  const setPromptData = useSetAtom(renderedPromptAtom)
  const areTestsRunning = useAtomValue(areTestsRunningAtom)
  const generatePreview = async () => {
    if (rt === undefined || ctx === undefined || selectedFn === undefined || selectedTc === undefined) {
      return
    }
    const newPreview = await selectedFn.render_prompt_for_test(rt, selectedTc.name, ctx, findMediaFile)
    setLastKnownPreview(newPreview)
    setPromptData(newPreview)
    return newPreview
  }

  const [lastKnownPreview, setLastKnownPreview] = useState<WasmPrompt | undefined>()

  const {
    data: preview,
    error,
    isLoading,
  } = useSWR(
    // areTestsRunning is added here since generatePreview iwll fail until the tests are done running. So we want this to re-run post-test-run. It fails because of WASM async issues (can't use the runtime while it's already in use. TBD how to fix)
    [rt, ctx, selectedFn, selectedTc, areTestsRunning],
    generatePreview,
  )

  if (isLoading && !preview) {
    if (lastKnownPreview) {
      return <RenderPrompt prompt={lastKnownPreview} testCase={selectedTc} />
    }
    return <Loader message='Loading...' />
  }

  if (error) {
    return <ErrorMessage error={error instanceof Error ? error.message : 'Unknown Error'} />
  }

  if (diagnostics.length > 0 && diagnostics.some((d) => d.type === 'error')) {
    return (
      <div className='relative'>
        {/* todo: maybe keep rendering the last known prompt? And make this a more condensed error banner in absolute position? */}
        <div className='p-3'>
          <div className='mb-2 text-sm font-medium text-red-500'>Syntax Error</div>
          <pre className='px-2 py-1 font-mono text-sm text-red-500 whitespace-pre-wrap rounded-lg'>
            <div className='space-y-2'>
              <div>{diagnostics.filter((d: WasmError) => d.type === 'error').length} error(s):</div>
              {diagnostics
                .filter((d: WasmError) => d.type === 'error')
                .map((d, i) => (
                  <div key={i}>- {d.message}</div>
                ))}
            </div>
          </pre>
        </div>
      </div>
    )
  }
  if (preview === undefined) {
    return <NoTestsContent />
  }

  return <RenderPrompt prompt={preview} testCase={selectedTc} />
}

export const NoTestsContent = () => {
  const { selectedFn } = useAtomValue(selectionAtom)
  const testSnippet = useAtomValue(functionTestSnippetAtom(selectedFn?.name ?? ''))
  const [copied, setCopied] = useState(false)

  const handleCopy = useCallback(() => {
    void navigator.clipboard.writeText(testSnippet ?? '')
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }, [testSnippet])

  return (
    <div className='flex flex-col justify-center items-center'>
      <div className='mb-4 text-sm font-medium text-muted-foreground'>Add a test to see the preview!</div>
      <div className='relative w-full max-w-2xl rounded-lg border border-border bg-muted'>
        <div className='absolute top-2 right-2'>
          <button
            onClick={handleCopy}
            className='px-2 py-1 text-xs font-medium rounded shadow-sm bg-background text-muted-foreground hover:bg-muted focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2'
          >
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>
        <pre className='overflow-x-auto p-4 font-mono text-sm text-balance text-foreground'>{testSnippet}</pre>
      </div>
    </div>
  )
}
