/// Content once a function has been selected.
import { useAppState } from './AppStateContext'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import React, { useState } from 'react'
import useSWR from 'swr'

import '@xyflow/react/dist/style.css'
import {
  wasmAtom,
  renderPromptAtom,
  selectedFunctionAtom,
  selectedRuntimeAtom,
  selectedTestCaseAtom,
  orchIndexAtom,
  expandImagesAtom,
  streamCurlAtom,
  rawCurlLoadable,
} from '../baml_wasm_web/EventListener'
import {
  // We _deliberately_ only import types from wasm, instead of importing the module: wasm load is async,
  // so we can only load wasm symbols through wasmAtom, not directly by importing wasm-schema-web
  type WasmChatMessagePartMedia,
  type WasmChatMessagePartMediaType,
} from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import TestResults from '../baml_wasm_web/test_uis/test_result'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'
import { TooltipProvider } from '../components/ui/tooltip'
import { PromptChunk } from './ImplPanel'
import FunctionTestSnippet from './TestSnippet'
import { Copy } from 'lucide-react'
import { Button } from '../components/ui/button'
import { CheckboxHeader } from './CheckboxHeader'
import { Switch } from '../components/ui/switch'
import { vscode } from '../utils/vscode'
import clsx from 'clsx'
import { ErrorBoundary } from 'react-error-boundary'

const handleCopy = (text: string) => () => {
  navigator.clipboard.writeText(text)
}

const CurlSnippet: React.FC = () => {
  const rawCurl = useAtomValue(rawCurlLoadable)
  const [streamCurl, setStreamCurl] = useAtom(streamCurlAtom)
  const [expandImages, setExpandImages] = useAtom(expandImagesAtom)

  // if (!wasm || !runtime || !func || !test_case) {
  //   return <div>Not yet ready</div>
  // }

  // const wasmCallContext = new wasm.WasmCallContext()
  // wasmCallContext.node_index = orch_index

  // const rawCurl = useSWR(
  //   { swr: 'CurlSnippet', runtime, func, test_case, orch_index, streamCurl, expandImages },
  //   async () => {
  //     return await func.render_raw_curl_for_test(
  //       runtime,
  //       test_case.name,
  //       wasmCallContext,
  //       streamCurl,
  //       expandImages,
  //       async (path: string) => {
  //         return await vscode.readFile(path)
  //       },
  //     )
  //   },
  // )

  return (
    <div>
      <div className='flex justify-end items-center p-2 space-x-2 rounded-md shadow-sm'>
        <label className='flex items-center mr-2 space-x-1'>
          <Switch
            className='data-[state=checked]:bg-vscode-button-background data-[state=unchecked]:bg-vscode-input-background'
            checked={streamCurl}
            onCheckedChange={setStreamCurl}
          />
          <span>Show Stream Request</span>
        </label>
        <label className='flex items-center mr-2 space-x-1'>
          <Switch
            className='data-[state=checked]:bg-vscode-button-background data-[state=unchecked]:bg-vscode-input-background'
            checked={expandImages}
            onCheckedChange={setExpandImages}
          />
          <span>Show fully expanded command</span>
        </label>
        <Button
          onClick={rawCurl.state === 'hasData' && rawCurl.data ? handleCopy(rawCurl.data) : () => {}}
          className='px-3 py-1 text-xs text-white bg-vscode-button-background hover:bg-vscode-button-hoverBackground'
        >
          <Copy size={16} />
        </Button>
      </div>
      {rawCurl.state === 'loading' ? (
        <div>Loading...</div>
      ) : (
        <PromptChunk
          text={(() => {
            switch (rawCurl.state) {
              case 'hasData':
                return rawCurl.data ?? ''
              case 'hasError':
                return `${rawCurl.error}`
            }
          })()}
          type={(() => {
            switch (rawCurl.state) {
              case 'hasData':
                return 'preview'
              case 'hasError':
                return 'error'
            }
          })()}
          client={{
            identifier: {
              end: 0,
              source_file: '',
              start: 0,
              value: 'Curl Request',
            },
            provider: '',
            model: '',
          }}
          showCopy={true}
        />
      )}
    </div>
  )
}

const WebviewMedia: React.FC<{ bamlMediaType: 'image' | 'audio'; media: WasmChatMessagePartMedia }> = ({
  bamlMediaType,
  media,
}) => {
  const wasm = useAtomValue(wasmAtom)
  const pathAsUri = useSWR({ swr: 'WebviewMedia', type: media.type, content: media.content }, async () => {
    if (!wasm) {
      return { error: 'wasm not loaded' }
    }

    switch (media.type) {
      case wasm.WasmChatMessagePartMediaType.File:
        // const uri = await vscode.readFile('', media.content)
        // // Do a manual check to assert that the image exists
        // if ((await fetch(uri, { method: 'HEAD' })).status !== 200) {
        //   throw new Error('file not found')
        // }
        return `file://${media.content}`
      case wasm.WasmChatMessagePartMediaType.Url:
        return media.content
      case wasm.WasmChatMessagePartMediaType.Error:
        return { error: media.content }
      default:
        return { error: 'unknown media type' }
    }
  })

  if (pathAsUri.error) {
    const error = typeof pathAsUri.error.message == 'string' ? pathAsUri.error.message : JSON.stringify(pathAsUri.error)
    return (
      <div className='px-2 py-1 rounded-lg bg-vscode-inputValidation-errorBackground'>
        <div>
          Error loading {bamlMediaType}: {error}
        </div>
      </div>
    )
  }

  if (pathAsUri.isLoading) {
    return <div>Loading {bamlMediaType}...</div>
  }

  const mediaUrl = pathAsUri.data as unknown as string

  return (
    <div className='p-1'>
      {(() => {
        switch (bamlMediaType) {
          case 'image':
            return (
              <a href={mediaUrl} target='_blank' rel='noopener noreferrer'>
                <img src={mediaUrl} className='max-h-[400px] max-w-[400px] object-left-top object-scale-down' />
              </a>
            )
          case 'audio':
            return (
              <audio controls>
                <source src={mediaUrl} />
                Your browser does not support the audio element.
              </audio>
            )
        }
      })()}
    </div>
  )
}

const PromptPreview: React.FC = () => {
  const promptPreview = useAtomValue(renderPromptAtom)
  const wasm = useAtomValue(wasmAtom)
  const { showCurlRequest } = useAppState()

  if (!promptPreview) {
    return (
      <div className='flex flex-col gap-2 justify-center items-center w-full h-full'>
        <span className='text-center'>No prompt preview available! Add a test to see it!</span>
        <FunctionTestSnippet />
      </div>
    )
  }

  if (typeof promptPreview === 'string') {
    return (
      <PromptChunk
        text={promptPreview}
        type='error'
        client={{
          identifier: {
            end: 0,
            source_file: '',
            start: 0,
            value: 'Error',
          },
          provider: 'baml-openai-chat',
          model: 'gpt-4',
        }}
      />
    )
  }

  if (showCurlRequest) {
    return <CurlSnippet />
  }

  return (
    <div className='flex flex-col gap-4 px-2 w-full h-full'>
      {promptPreview.as_chat()?.map((chat, idx) => (
        <div key={idx} className='flex flex-col'>
          <div className='flex flex-row'>{chat.role}</div>
          {chat.parts.map((part, idx) => {
            if (part.is_text())
              return (
                <PromptChunk
                  key={idx}
                  text={part.as_text()!}
                  client={{
                    identifier: {
                      end: 0,
                      source_file: '',
                      start: 0,
                      value: promptPreview.client_name,
                    },
                    provider: 'baml-openai-chat',
                    model: 'gpt-4',
                  }}
                />
              )
            if (part.is_image()) {
              const media = part.as_media()
              if (!media) return <div key={idx}>Error loading image: this chat message part is not media</div>
              if (media.type === wasm?.WasmChatMessagePartMediaType.Error)
                return <div key={idx}>Error loading image 1: {media.content}</div>
              return <WebviewMedia key={idx} bamlMediaType='image' media={media} />
            }
            if (part.is_audio()) {
              const media = part.as_media()
              if (!media) return <div key={idx}>Error loading audio: this chat message part is not media</div>
              if (media.type === wasm?.WasmChatMessagePartMediaType.Error)
                return <div key={idx}>Error loading audio 1: {media.content}</div>
              return <WebviewMedia key={idx} bamlMediaType='audio' media={media} />
            }
            return null
          })}
        </div>
      ))}
    </div>
  )
}

const FunctionPanel: React.FC = () => {
  const selectedFunc = useAtomValue(selectedFunctionAtom)
  const { showTestResults } = useAppState()

  if (!selectedFunc) {
    const bamlFunctionSnippet = `
function ClassifyConversation(convo: string[]) -> Topic[] {
  client GPT4
  prompt #"
    Classify the CONVERSATION.

    {{ ctx.output_format }}

    CONVERSATION:
    {% for c in convo %}
    {{ c }}
    {% endfor %}
  "#
}

enum Topic {
  TechnicalSupport
  Sales
  CustomerService
  Other
}
  `.trim()
    return (
      <div className='flex flex-col gap-2 justify-center items-center w-full h-full'>
        No functions found! You can create a new function like:
        <pre className='p-2 text-xs rounded-sm bg-vscode-input-background'>{bamlFunctionSnippet}</pre>
      </div>
    )
  }

  return (
    <div
      className='flex overflow-auto flex-col w-full'
      style={{
        height: 'calc(100vh - 80px)',
      }}
    >
      <TooltipProvider>
        <ResizablePanelGroup direction='vertical' className='h-full'>
          <ResizablePanel id='top-panel' className='flex px-1 w-full' defaultSize={50}>
            <div className='w-full'>
              <ResizablePanelGroup direction='horizontal' className='pb-4 h-full'>
                <div className='w-full h-full'>
                  <CheckboxHeader />
                  <div className='overflow-y-auto relative w-full' style={{ height: 'calc(100% - 32px)' }}>
                    <PromptPreview />
                  </div>
                </div>
              </ResizablePanelGroup>

              {/* </Allotment> */}
            </div>
          </ResizablePanel>
          {showTestResults && (
            <>
              <ResizableHandle withHandle={false} className='bg-vscode-panel-border' />
              <ResizablePanel
                minSize={10}
                className='flex px-0 py-2 pb-3 h-full border-t border-vscode-textSeparator-foreground'
              >
                <ErrorBoundary fallback={<div>Error loading test results</div>}>
                  <TestResults />
                </ErrorBoundary>
              </ResizablePanel>
            </>
          )}
        </ResizablePanelGroup>
      </TooltipProvider>
    </div>
  )
}

export default FunctionPanel
