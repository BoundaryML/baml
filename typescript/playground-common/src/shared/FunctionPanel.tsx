/// Content once a function has been selected.

import { useAtomValue } from 'jotai'
import { useId, useRef } from 'react'
import { renderPromptAtom, selectedFunctionAtom } from '../baml_wasm_web/EventListener'
import TestResults from '../baml_wasm_web/test_uis/test_result'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'
import { TooltipProvider } from '../components/ui/tooltip'
import { Snippet } from './ImplPanel'
import FunctionTestSnippet from './TestSnippet'

const PromptPreview: React.FC = () => {
  const promptPreview = useAtomValue(renderPromptAtom)
  if (!promptPreview) {
    return (
      <div className='flex flex-col items-center justify-center w-full h-full gap-2'>
        <span className='text-center'>No prompt preview available! Add a test to see it!</span>
        <FunctionTestSnippet />
      </div>
    )
  }

  if (typeof promptPreview === 'string')
    return (
      <Snippet
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

  return (
    <div className='flex flex-col w-full h-full gap-4 px-2'>
      {promptPreview.as_chat()?.map((chat, idx) => (
        <div key={idx} className='flex flex-col'>
          <div className='flex flex-row'>{chat.role}</div>
          {chat.parts.map((part, idx) => {
            if (part.is_text())
              return (
                <Snippet
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
            if (part.is_image()) return <img key={idx} src={part.as_image()} className='max-w-40' />
            return null
          })}
        </div>
      ))}
    </div>
  )
}

const FunctionPanel: React.FC = () => {
  const selectedFunc = useAtomValue(selectedFunctionAtom)

  const id = useId()
  const refs = useRef()

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
              <ResizablePanelGroup direction='horizontal' className='h-full'>
                <div className='relative w-full h-full overflow-y-auto'>
                  <PromptPreview />
                </div>
              </ResizablePanelGroup>

              {/* </Allotment> */}
            </div>
          </ResizablePanel>
          <ResizableHandle withHandle={false} className='bg-vscode-panel-border' />
          <ResizablePanel
            minSize={10}
            className='flex h-full px-0 py-2 mb-6 border-t border-vscode-textSeparator-foreground'
          >
            <TestResults />
          </ResizablePanel>
        </ResizablePanelGroup>
      </TooltipProvider>
    </div>
  )
}

export default FunctionPanel
