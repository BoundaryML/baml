import { useEffect, useState, useMemo } from 'react'

import './App.css'

import { ASTProvider } from './shared/ASTProvider'
import FunctionPanel from './shared/FunctionPanel'
import { FunctionSelector } from './shared/Selectors'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import CustomErrorBoundary from './utils/ErrorFallback'

function App() {
  return (
    <CustomErrorBoundary>
      <ASTProvider>
        <div className="sticky right-0 z-10 top-2 text-end">
          <VSCodeLink href="https://docs.boundaryml.com">Docs</VSCodeLink>
        </div>
        <div className="relative flex flex-col gap-2">
          <div className="flex flex-row justify-between">
            <div className="flex flex-row items-center gap-1">
              <b>Function</b> <FunctionSelector />
            </div>
          </div>
          <FunctionPanel />
        </div>
      </ASTProvider>
    </CustomErrorBoundary>
  )
}

export default App
