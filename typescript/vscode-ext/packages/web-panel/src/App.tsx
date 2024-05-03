import { useEffect, useState, useMemo, useContext } from 'react'

import './App.css'
import 'allotment/dist/style.css'

import { EventListener } from './baml_wasm_web/EventListener'
// import { ASTContext, ASTProvider } from './shared/ASTProvider'
import FunctionPanel from './shared/FunctionPanel'
import { FunctionSelector } from './shared/Selectors'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import CustomErrorBoundary from './utils/ErrorFallback'
import { Separator } from './components/ui/separator'
import { Button } from './components/ui/button'
import { FlaskConical, FlaskConicalOff } from 'lucide-react'
import { useSelections } from './shared/hooks'
import { ProjectToggle } from './shared/ProjectPanel'

const TestToggle = () => {
  // const { setSelection } = useContext(ASTContext)
  const { showTests } = useSelections()

  return (
    <Button
      variant="outline"
      className="p-1 text-xs w-fit h-fit border-vscode-textSeparator-foreground"
      onClick={() => { }}
    >
      {showTests ? 'Hide tests' : 'Show tests'}
    </Button>
  )
}

function App() {
  return (
    <CustomErrorBoundary>
      <EventListener>
        <div className="absolute z-10 flex flex-col items-end gap-1 right-1 top-2 text-end">
          <TestToggle />
          <VSCodeLink href="https://docs.boundaryml.com">Docs</VSCodeLink>
        </div>
        <div className="flex flex-col gap-2 px-2 pb-4">
          <FunctionSelector />
          <Separator className="bg-vscode-textSeparator-foreground" />
          <FunctionPanel />
        </div>
      </EventListener>
    </CustomErrorBoundary>
  )
}

export default App
