import { useEffect, useState, useMemo } from 'react'

import './App.css'
import 'allotment/dist/style.css'

import { ASTProvider } from './shared/ASTProvider'
import FunctionPanel from './shared/FunctionPanel'
import { FunctionSelector } from './shared/Selectors'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import CustomErrorBoundary from './utils/ErrorFallback'
import { Separator } from './components/ui/separator'
import { Button } from './components/ui/button'
import { FlaskConical, FlaskConicalOff } from 'lucide-react'

function App() {
  const [selected, setSelected] = useState<boolean>(true)

  return (
    <CustomErrorBoundary>
      <ASTProvider>
        <div className="absolute right-0 z-10 top-2 text-end flex flex-col gap-1">
          <VSCodeLink href="https://docs.boundaryml.com">Docs</VSCodeLink>
          <Button variant="ghost" className="p-0" onClick={() => setSelected((p) => !p)}>
            {selected ? <FlaskConical className="w-4 h-4" /> : <FlaskConicalOff className="w-4 h-4" />}
          </Button>
        </div>
        <div className="flex flex-col gap-2 px-2">
          <FunctionSelector />
          <Separator className="bg-vscode-textSeparator-foreground" />
          <FunctionPanel showTests={selected} />
        </div>
      </ASTProvider>
    </CustomErrorBoundary>
  )
}

export default App
