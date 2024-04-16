/// Content once a function has been selected.

import { Separator } from '../components/ui/separator'
import { TestCaseSelector } from './Selectors'
import { useSelections } from './hooks'
import { VSCodeDivider, VSCodePanels } from '@vscode/webview-ui-toolkit/react'
import TestCasePanel from './TestCasePanel'
import ImplPanel from './ImplPanel'
import { useContext, useEffect, useId, useState } from 'react'
import { ASTContext } from './ASTProvider'
import TypeComponent from './TypeComponent'
import TestResultPanel from './TestResultOutcomes'
import { ScrollArea, ScrollBar } from '../components/ui/scroll-area'
import { Button } from '../components/ui/button'
import { FlaskConical } from 'lucide-react'
import clsx from 'clsx'
import { TooltipProvider } from '../components/ui/tooltip'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'

const FunctionPanel: React.FC = () => {
  const {
    selections: { showTests },
    setSelection,
  } = useContext(ASTContext)
  const { func, impl } = useSelections()

  if (!func) return <div className="flex flex-col">No function selected</div>
  const { test_results } = useSelections()
  const results = test_results ?? []
  const id = useId()

  let impls = <div />
  if (!impl) {
    impls = <div />
  } else if (func.impls.length === 1) {
    impls = <ImplPanel showTab={false} impl={func.impls[0]} />
  } else {
    impls = (
      <VSCodePanels
        activeid={`tab-${func.name.value}-${impl.name.value}`}
        onChange={(e) => {
          const selected: string | undefined = (e.target as any)?.activetab?.id
          if (selected && selected.startsWith(`tab-${func.name.value}-`)) {
            setSelection(undefined, undefined, selected.split('-', 3)[2], undefined, undefined)
          }
        }}
      >
        {func.impls.map((impl) => (
          <ImplPanel showTab={true} impl={impl} key={`${func.name.value}-${impl.name.value}`} />
        ))}
      </VSCodePanels>
    )
  }

  let topPanelSize = 100
  if (showTests) {
    if (test_results && test_results.length > 0) {
      topPanelSize = 40
    } else {
      topPanelSize = 85
    }
  }
  let testResultId = test_results ? test_results[0].status : ''

  console.log('testResultId', testResultId, topPanelSize)
  return (
    <div
      className="flex flex-col w-full overflow-auto"
      style={{
        height: 'calc(100vh - 80px)',
      }}
    >
      <TooltipProvider>
        {/* <Allotment vertical> */}
        <ResizablePanelGroup
          key={testResultId}
          direction="vertical"
          className="h-full"
          id={id + showTests.valueOf() + testResultId}
        >
          <ResizablePanel className="flex w-full " defaultSize={topPanelSize}>
            <div
              className={clsx('w-full ', {
                // 'basis-[60%]': showTests && results.length > 0,
                // 'basis-[100%]': !showTests,
                // 'basis-[85%]': showTests && !(results.length > 0),
              })}
            >
              {/* <Allotment className="h-full"> */}
              <ResizablePanelGroup direction="horizontal" className="h-full">
                {impl && (
                  <ResizablePanel defaultSize={50} className="px-0">
                    <div className="relative h-full">
                      <ScrollArea type="always" className="flex w-full h-full pr-3 ">
                        {impls}
                      </ScrollArea>
                    </div>
                  </ResizablePanel>
                )}
                <ResizableHandle withHandle={true} className="bg-vscode-panel-border" />
                <ResizablePanel minSize={50} className="pl-2 pr-0.5" hidden={!showTests}>
                  {/* <Allotment.Pane className="pl-2 pr-0.5" minSize={200} visible={showTests}> */}
                  <div className="flex flex-col h-full overflow-y-auto overflow-x-clip">
                    {/* On windows this scroll area extends beyond the wanted width, so we just use a normal scrollbar here vs using ScrollArea*/}
                    <TestCasePanel func={func} />
                  </div>
                </ResizablePanel>
              </ResizablePanelGroup>

              {/* </Allotment> */}
            </div>
          </ResizablePanel>
          <ResizableHandle withHandle={true} className="bg-vscode-panel-border" />
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
