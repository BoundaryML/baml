import { useEffect, useState, useMemo } from 'react'

import './App.css'
import 'allotment/dist/style.css'

import { ASTProvider } from './shared/ASTProvider'
import FunctionPanel from './shared/FunctionPanel'
import { FunctionSelector } from './shared/Selectors'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import CustomErrorBoundary from './utils/ErrorFallback'

function App() {
  return (
    <CustomErrorBoundary>
      <ASTProvider>
        <div className="absolute right-0 z-10 top-2 text-end">
          <VSCodeLink href="https://docs.boundaryml.com">Docs</VSCodeLink>
        </div>
        <div className="flex flex-col gap-2">
          <FunctionSelector />
          <hr />
          <FunctionPanel />
        </div>
      </ASTProvider>
    </CustomErrorBoundary>
  )
}

export default App
