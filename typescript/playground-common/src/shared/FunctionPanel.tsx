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

const PromptPreview: React.FC = () => {
  const propmtPreview = useAtomValue(renderPromptAtom)
  if (!propmtPreview) return <div className='flex flex-col'>No prompt preview!</div>

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

  if (!selectedFunc)
    return <div className='flex flex-col'>No function selected. Create or select a function to get started</div>

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
                <ResizablePanel defaultSize={60} className='overflow-y-auto'>
                  <div className='relative w-full h-full overflow-y-auto'>
                    <PromptPreview />
                  </div>
                </ResizablePanel>
                <ResizableHandle withHandle={false} className='bg-vscode-panel-border' />
                <ResizablePanel minSize={20} className='pl-2'>
                  <div className='flex flex-col h-full overflow-x-clip overflow-y-auto'>
                    {/* On windows this scroll area extends beyond the wanted width, so we just use a normal scrollbar here vs using ScrollArea*/}
                    <TestCasePanel />
                  </div>
                </ResizablePanel>
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
