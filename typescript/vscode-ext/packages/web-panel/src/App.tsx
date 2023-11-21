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
        <div className="flex flex-col gap-2">
          <div className="flex flex-row justify-between">
            <div className="flex flex-row gap-1 items-center">
              <b>Function</b> <FunctionSelector />
            </div>
            <VSCodeLink className="flex ml-auto h-7" href="https://docs.trygloo.com">
              Docs
            </VSCodeLink>
          </div>
          <FunctionPanel />
        </div>
      </ASTProvider>
    </CustomErrorBoundary>
  )
}

export default App
