import { Suspense, useEffect, useState, useMemo, useContext } from 'react'

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
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from './components/ui/dialog'
import { FlaskConical, FlaskConicalOff } from 'lucide-react'
import { ProjectToggle } from './shared/ProjectPanel'
import SettingsDialog, { showSettingsAtom } from './shared/SettingsDialog'
import { atom, useAtom } from 'jotai'
import { DevTools } from 'jotai-devtools'
import 'jotai-devtools/styles.css'
import { Settings } from 'lucide-react'
import { showTestsAtom } from './baml_wasm_web/test_uis/testHooks'

const TestToggle = () => {
  // const { setSelection } = useContext(ASTContext)
  const [showTests, setShowTests] = useAtom(showTestsAtom)

  return (
    <Button
      variant='outline'
      className='p-1 text-xs w-fit h-4 border-vscode-textSeparator-foreground'
      onClick={() => setShowTests((prev) => !prev)}
    >
      {showTests ? 'Hide tests' : 'Show tests'}
    </Button>
  )
}

function App() {
  const [showSettings, setShowSettings] = useAtom(showSettingsAtom)
  return (
    <CustomErrorBoundary>
      <DevTools />
      <Suspense fallback={<div>Loading...</div>}>
        <EventListener>
          <div className='absolute z-10 flex flex-row items-end gap-1 right-1 top-2 text-end'>
            <TestToggle />
            <VSCodeLink href='https://docs.boundaryml.com'>Docs</VSCodeLink>
            <Button className='h-4' onClick={() => setShowSettings(true)}>
              <Settings className='h-4' />
            </Button>
          </div>
          <SettingsDialog />
          <div className='flex flex-col gap-2 px-2 pb-4'>
            <FunctionSelector />
            <Separator className='bg-vscode-textSeparator-foreground' />
            <FunctionPanel />
          </div>
        </EventListener>
      </Suspense>
    </CustomErrorBoundary>
  )
}

export default App
