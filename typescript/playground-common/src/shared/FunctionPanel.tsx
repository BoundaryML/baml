/// Content once a function has been selected.
import { useAppState } from './AppStateContext'
import { useAtomValue, useSetAtom } from 'jotai'
import React, { useState } from 'react'

import '@xyflow/react/dist/style.css'
import {
  renderPromptAtom,
  selectedFunctionAtom,
  curlAtom,
  streamCurl,
  expandImages,
} from '../baml_wasm_web/EventListener'
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

const handleCopy = (text: string) => () => {
  navigator.clipboard.writeText(text)
}

const CurlSnippet: React.FC = () => {
  const rawCurl = useAtomValue(curlAtom) ?? 'Loading...'

  return (
    <div>
      <div className='flex items-center justify-end p-2 space-x-2 rounded-md shadow-sm'>
        <label className='flex items-center mr-2 space-x-1'>
          <Switch
            className='data-[state=checked]:bg-vscode-button-background data-[state=unchecked]:bg-vscode-input-background'
            checked={useAtomValue(streamCurl)}
            onCheckedChange={useSetAtom(streamCurl)}
          />
          <span>View Stream Request</span>
        </label>
        <label className='flex items-center mr-2 space-x-1'>
          <Switch
            className='data-[state=checked]:bg-vscode-button-background data-[state=unchecked]:bg-vscode-input-background'
            checked={useAtomValue(expandImages)}
            onCheckedChange={useSetAtom(expandImages)}
          />
          <span>Expand images as base64</span>
        </label>
        <Button
          onClick={handleCopy(rawCurl)}
          className='px-3 py-1 text-xs text-white bg-vscode-button-background hover:bg-vscode-button-hoverBackground'
        >
          <Copy size={16} />
        </Button>
      </div>
      <PromptChunk
        text={rawCurl}
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
    </div>
  )
}

type WasmChatMessagePartMedia =
  | {
      type: 'url'
      url: string
    }
  | {
      type: 'path'
      path: string
    }
  | {
      type: 'error'
      error: string
    }

const WebviewImage: React.FC<{ image?: WasmChatMessagePartMedia }> = ({ image }) => {
  const [fileUrl, setFileUrl] = useState<string | null>(null)

  if (!image) {
    return <div>BAML internal error: chat message part is not image</div>
  }

  let imageUrl = null

  switch (image.type) {
    case 'url':
      imageUrl = image.url
      break
    case 'path':
      ;(async () => {
        const fileUrl = await vscode.asWebviewUri('', image.path)
        setFileUrl(fileUrl)
      })()
      imageUrl = fileUrl
      break
    case 'error':
      return <div>Error loading image: {image.error}</div>
  }

  if (!imageUrl) {
    return <div>Loading image...</div>
  }

  return image.type === 'path' ? (
    <img src={imageUrl} alt={image.path} className='max-h-[400px] max-w-[400px] object-left-top object-scale-down' />
  ) : (
    <a href={imageUrl} target='_blank' rel='noopener noreferrer'>
      <img src={imageUrl} className='max-h-[400px] max-w-[400px] object-left-top object-scale-down' />
    </a>
  )
}

const WebviewAudio: React.FC<{ audio?: WasmChatMessagePartMedia }> = ({ audio }) => {
  const [audioUrl, setAudioUrl] = useState<string | null>(audio && audio.type === 'url' ? audio.url : null)
  if (!audio) {
    return <div>BAML internal error: chat message part is not audio</div>
  }
  if (audio.type === 'path') {
    ;(async () => {
      const imageUrl = await vscode.asWebviewUri('', audio.path)
      setAudioUrl(imageUrl)
    })()
  }

  if (audio.type === 'error') {
    return <div>Error loading audio: {audio.error}</div>
  }

  if (!audioUrl) {
    return <div>Loading audio...</div>
  }

  return (
    <audio controls>
      <source src={audioUrl} />
      Your browser does not support the audio element.
    </audio>
  )
}

const PromptPreview: React.FC = () => {
  const promptPreview = useAtomValue(renderPromptAtom)
  const { showCurlRequest } = useAppState()

  if (!promptPreview) {
    return (
      <div className='flex flex-col items-center justify-center w-full h-full gap-2'>
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
    <div className='flex flex-col w-full h-full gap-4 px-2'>
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
            if (part.is_image()) return <WebviewImage key={idx} image={part.as_media()} />
            if (part.is_audio()) return <WebviewAudio key={idx} audio={part.as_media()} />
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
      <div className='flex flex-col items-center justify-center w-full h-full gap-2'>
        No functions found! You can create a new function like:
        <pre className='p-2 text-xs rounded-sm bg-vscode-input-background'>{bamlFunctionSnippet}</pre>
      </div>
    )
  }

  return (
    <div
      className='flex flex-col w-full overflow-auto'
      style={{
        height: 'calc(100vh - 80px)',
      }}
    >
      <TooltipProvider>
        <ResizablePanelGroup direction='vertical' className='h-full'>
          <ResizablePanel id='top-panel' className='flex w-full px-1' defaultSize={50}>
            <div className='w-full'>
              <ResizablePanelGroup direction='horizontal' className='h-full pb-4'>
                <div className='w-full h-full'>
                  <CheckboxHeader />
                  <div className='relative w-full overflow-y-auto' style={{ height: 'calc(100% - 32px)' }}>
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
                className='flex h-full px-0 py-2 pb-3 border-t border-vscode-textSeparator-foreground'
              >
                <TestResults />
              </ResizablePanel>
            </>
          )}
        </ResizablePanelGroup>
      </TooltipProvider>
    </div>
  )
}

export default FunctionPanel
