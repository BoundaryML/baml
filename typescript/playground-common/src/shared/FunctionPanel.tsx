/// Content once a function has been selected.

import { TestResult } from '@baml/common'
import { VSCodePanels } from '@vscode/webview-ui-toolkit/react'
import clsx from 'clsx'
import { createRef, useContext, useEffect, useId, useRef } from 'react'
import { ImperativePanelHandle } from 'react-resizable-panels'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'
import { TooltipProvider } from '../components/ui/tooltip'
import { ASTContext } from './ASTProvider'
import ImplPanel from './ImplPanel'
import TestCasePanel from './TestCasePanel'
import TestResultPanel from './TestResultOutcomes'
import { useSelections } from './hooks'
import { useAtomValue } from 'jotai'
import { renderPromptAtom, selectedFunctionAtom } from '@/baml_wasm_web/EventListener'

function getTopPanelSize(showTests: boolean, test_results: TestResult[] | undefined): number {
  if (showTests) {
    if (test_results && test_results.length > 0) {
      return 40
    } else {
      return 85
    }
  }
  return 100
}

const PromptPreview: React.FC = () => {
  const propmtPreview = useAtomValue(renderPromptAtom);
  if (!propmtPreview) return <div className="flex flex-col">No prompt preview!</div>

  if (typeof propmtPreview === 'string') return <div className="flex flex-col">{propmtPreview}</div>

  return (
    <div className="flex flex-col w-full h-full gap-4">
      {propmtPreview.as_chat()?.map((chat, idx) => (
        <div key={idx} className="flex flex-col">
          <div className='flex flex-row'>{chat.role}</div>
          {chat.parts.map((part, idx) => {
            if (part.is_text()) return <div key={idx} className='flex flex-row'>{part.as_text()}</div>
            if (part.is_image()) return <img key={idx} src={part.as_image()} className='max-w-40' />
            return null;
          })}
        </div>
      ))}
    </div>
  )
}

const FunctionPanel: React.FC = () => {
  const showTests = true
  const selectedFunc = useAtomValue(selectedFunctionAtom);

  const { test_results } = useSelections()
  const results = test_results ?? []
  const id = useId()
  const refs = useRef()
  const testResultId = test_results ? test_results[0]?.status : ''
  const ref = createRef<ImperativePanelHandle>()

  useEffect(() => {
    let topPanelSize = getTopPanelSize(showTests, test_results)
    if (ref.current) {
      ref.current.resize(topPanelSize)
    }
  }, [showTests, testResultId])

  if (!selectedFunc)
    return <div className="flex flex-col">No function selected. Create or select a function to get started</div>

  let topPanelSize = getTopPanelSize(showTests, test_results)

  return (
    <div
      className="flex flex-col w-full overflow-auto"
      style={{
        height: 'calc(100vh - 80px)',
      }}
    >
      <TooltipProvider>
        <ResizablePanelGroup direction="vertical" className="h-full">
          <ResizablePanel id="top-panel" ref={ref} className="flex w-full " defaultSize={topPanelSize}>
            <div className="w-full">
              <ResizablePanelGroup direction="horizontal" className="h-full">
                <ResizablePanel defaultSize={60} className="px-0 overflow-y-auto">
                  <div className="relative w-full h-full overflow-y-auto">
                    {/* <ScrollArea type="auto" className="flex w-full h-full pr-3 "> */}
                    <div className="flex w-full h-full">
                      <PromptPreview />
                    </div>
                    {/* </ScrollArea> */}
                  </div>
                </ResizablePanel>
                <ResizableHandle withHandle={false} className="bg-vscode-panel-border" />
                <ResizablePanel minSize={20} className="pl-2 pr-0.5" hidden={!showTests}>
                  {/* <Allotment.Pane className="pl-2 pr-0.5" minSize={200} visible={showTests}> */}
                  <div className="flex flex-col h-full overflow-y-auto overflow-x-clip">
                    {/* On windows this scroll area extends beyond the wanted width, so we just use a normal scrollbar here vs using ScrollArea*/}
                    <TestCasePanel />
                  </div>
                </ResizablePanel>
              </ResizablePanelGroup>

              {/* </Allotment> */}
            </div>
          </ResizablePanel>
          <ResizableHandle withHandle={false} className="bg-vscode-panel-border" />
          <ResizablePanel minSize={10} className="px-0 overflow-y-auto">
            <div
              className={clsx('py-2 border-t h-full border-vscode-textSeparator-foreground', {
                flex: showTests,
                hidden: !showTests,
              })}
            >
              <div className="w-full h-full">
                <TestResultPanel />
              </div>
            </div>
          </ResizablePanel>
        </ResizablePanelGroup>
      </TooltipProvider>
    </div>
  )
}

export default FunctionPanel
