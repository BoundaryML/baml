/// Content once a function has been selected.

import type { TestResult } from '@baml/common'
import { VSCodePanels } from '@vscode/webview-ui-toolkit/react'
import clsx from 'clsx'
import { useAtomValue } from 'jotai'
import { createRef, useContext, useEffect, useId, useMemo, useRef } from 'react'
import type { ImperativePanelHandle } from 'react-resizable-panels'
import { renderPromptAtom, selectedFunctionAtom } from '../baml_wasm_web/EventListener'
import TestResults from '../baml_wasm_web/test_uis/test_result'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'
import { TooltipProvider } from '../components/ui/tooltip'
import { ASTContext } from './ASTProvider'
import ImplPanel, { Snippet } from './ImplPanel'
import TestCasePanel from './TestCasePanel'
import TestResultPanel from './TestResultOutcomes'
import FunctionTestSnippet from './TestSnippet'

const PromptPreview: React.FC = () => {
  const propmtPreview = useAtomValue(renderPromptAtom)
  if (!propmtPreview) {
    return (
      <div className='flex flex-col w-full h-full items-center justify-center gap-2'>
        <span className='text-center'>No prompt preview available!</span>
        <FunctionTestSnippet />
      </div>
    )
  }

  if (typeof propmtPreview === 'string')
    return (
      <Snippet
        text={propmtPreview}
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
      {propmtPreview.as_chat()?.map((chat, idx) => (
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
                      value: propmtPreview.client_name,
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
      <div className='flex flex-col w-full h-full items-center justify-center gap-2'>
        No functions found! You can create a new function like:
        <pre className='bg-vscode-input-background p-2 rounded-sm text-xs'>{bamlFunctionSnippet}</pre>
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
          <ResizablePanel id='top-panel' className='flex w-full px-2' defaultSize={50}>
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
            className='px-0 py-2 h-full border-t border-vscode-textSeparator-foreground flex mb-6'
          >
            <TestResults />
          </ResizablePanel>
        </ResizablePanelGroup>
      </TooltipProvider>
    </div>
  )
}

export default FunctionPanel
