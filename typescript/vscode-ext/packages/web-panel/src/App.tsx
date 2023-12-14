import { useEffect, useState, useMemo, useContext } from 'react'

import './App.css'
import 'allotment/dist/style.css'

import { ASTContext, ASTProvider } from './shared/ASTProvider'
import FunctionPanel from './shared/FunctionPanel'
import { FunctionSelector } from './shared/Selectors'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import CustomErrorBoundary from './utils/ErrorFallback'
import { Separator } from './components/ui/separator'
import { Button } from './components/ui/button'
import { FlaskConical, FlaskConicalOff } from 'lucide-react'
import { useSelections } from './shared/hooks'

const TestToggle = () => {
  const { setSelection } = useContext(ASTContext)
  const { showTests } = useSelections()

  return (
    <Button
      variant="outline"
      className="p-1 w-fit h-fit text-xs"
      onClick={() => setSelection(undefined, undefined, undefined, !showTests)}
    >
      {showTests ? 'Hide tests' : 'Show tests'}
    </Button>
  )
}

function App() {
  const [selected, setSelected] = useState<boolean>(true)

  return (
    <CustomErrorBoundary>
      <ASTProvider>
        <div className="absolute right-1 z-10 flex flex-col gap-1 top-2 text-end items-end">
          <TestToggle />
          <VSCodeLink href="https://docs.boundaryml.com">Docs</VSCodeLink>
        </div>
        <div className="flex flex-col gap-2 px-2">
          <FunctionSelector />
          <Separator className="bg-vscode-textSeparator-foreground" />
          <FunctionPanel />
        </div>
      </ASTProvider>
    </CustomErrorBoundary>
  )
}

export default App
