/// Content once a function has been selected.

import { Separator } from '@/components/ui/separator'
import { TestCaseSelector } from './Selectors'
import { useSelections } from './hooks'
import { VSCodeDivider, VSCodePanels } from '@vscode/webview-ui-toolkit/react'
import TestCasePanel from './TestCasePanel'
import ImplPanel from './ImplPanel'
import { useContext, useEffect } from 'react'
import { ASTContext } from './ASTProvider'

const FunctionPanel: React.FC = () => {
  const { setSelection } = useContext(ASTContext)
  const { func, impl } = useSelections()

  if (!func) return <div className="flex flex-col">No function selected</div>

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-row gap-1">
        <b>Test Case</b> <TestCaseSelector />
      </div>
      <TestCasePanel func={func} />
      <VSCodeDivider role="separator" />
      {impl && (
        <VSCodePanels
          className="w-full"
          activeid={`tab-${func.name.value}-${impl.name.value}`}
          onChange={(e) => {
            const selected: string | undefined = (e.target as any)?.activetab?.id
            selected && setSelection(undefined, selected.split('-', 3)[2], undefined)
          }}
        >
          {func.impls.map((impl) => (
            <ImplPanel impl={impl} key={`${func.name.value}-${impl.name.value}`} />
          ))}
        </VSCodePanels>
      )}
    </div>
  )
}

export default FunctionPanel
