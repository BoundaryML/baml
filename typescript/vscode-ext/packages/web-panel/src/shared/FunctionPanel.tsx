/// Content once a function has been selected.

import { Separator } from '@/components/ui/separator'
import { TestCaseSelector } from './Selectors'
import { useSelections } from './hooks'
import { VSCodeDivider, VSCodePanels } from '@vscode/webview-ui-toolkit/react'
import TestCasePanel from './TestCasePanel'
import ImplPanel from './ImplPanel'
import { useContext, useEffect } from 'react'
import { ASTContext } from './ASTProvider'
import TypeComponent from './TypeComponent'
import { Allotment } from 'allotment'
import TestResultPanel from './TestResultOutcomes'
import { ScrollArea } from '@/components/ui/scroll-area'

const FunctionPanel: React.FC = () => {
  const { setSelection } = useContext(ASTContext)
  const { func, impl } = useSelections()

  if (!func) return <div className="flex flex-col">No function selected</div>

  return (
    <div className="w-full h-screen overflow-auto">
      <Allotment vertical>
        <Allotment.Pane>
          <Allotment className="h-full ">
            {impl && (
              <Allotment.Pane className="px-2" minSize={200}>
                <VSCodePanels
                  activeid={`tab-${func.name.value}-${impl.name.value}`}
                  onChange={(e) => {
                    const selected: string | undefined = (e.target as any)?.activetab?.id
                    if (selected && selected.startsWith(`tab-${func.name.value}-`)) {
                      setSelection(undefined, selected.split('-', 3)[2], undefined)
                    }
                  }}
                >
                  {func.impls.map((impl) => (
                    <ImplPanel impl={impl} key={`${func.name.value}-${impl.name.value}`} />
                  ))}
                </VSCodePanels>
              </Allotment.Pane>
            )}
            <Allotment.Pane className="px-2" minSize={200}>
              <div className="h-full">
                <ScrollArea type="always" className="flex w-full h-full pr-3">
                  <>
                    {/* <div className="flex flex-row gap-1">
                      <b>Test Case</b> <TestCaseSelector />
                    </div> */}
                    <TestCasePanel func={func} />
                  </>
                </ScrollArea>
              </div>
            </Allotment.Pane>
          </Allotment>
        </Allotment.Pane>
        <Allotment.Pane className="py-2">
          <TestResultPanel />
        </Allotment.Pane>
      </Allotment>
    </div>
  )
}

export default FunctionPanel
