import { Suspense, useContext, useEffect, useMemo, useState } from 'react'

import './App.css'
import 'allotment/dist/style.css'

import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import { useAtom, useSetAtom } from 'jotai'
import { DevTools } from 'jotai-devtools'
import { FlaskConical, FlaskConicalOff, Compass } from 'lucide-react'
import { EventListener } from './baml_wasm_web/EventListener'
import { Button } from './components/ui/button'
import { Separator } from './components/ui/separator'
// import { ASTContext, ASTProvider } from './shared/ASTProvider'
import FunctionPanel from './shared/FunctionPanel'
import { FunctionSelector } from './shared/Selectors'
import SettingsDialog, { ShowSettingsButton, showSettingsAtom } from './shared/SettingsDialog'
import CustomErrorBoundary from './utils/ErrorFallback'
import 'jotai-devtools/styles.css'
import { SettingsIcon } from 'lucide-react'
import { BamlProjectsGroupings, loadExampleProjects } from './lib/utils'
import { Snippets } from './shared/Snippets'
import { Dialog, DialogTrigger, DialogContent } from './components/ui/dialog'
function App() {
  return (
    <CustomErrorBoundary>
      <DevTools />
      <Suspense fallback={<div>Loading...</div>}>
        <EventListener>
          <div className='absolute z-10 flex flex-row items-center justify-center gap-1 right-1 top-2 text-end'>
            <Dialog>
              <DialogTrigger>
                <Button
                  variant={'ghost'}
                  className='flex flex-row items-center px-2 py-1 text-sm whitespace-pre-wrap bg-indigo-600 hover:bg-indigo-500 h-fit gap-x-2 text-vscode-button-foreground mr-2'
                >
                  <Compass size={16} strokeWidth={2} />
                  <span className='whitespace-nowrap'>Explore Examples</span>
                </Button>
              </DialogTrigger>

              <DialogContent className='fullWidth min-w-full h-full border-zinc-900 bg-zinc-900'>
                <Snippets></Snippets>
              </DialogContent>
            </Dialog>

            <VSCodeLink href='https://docs.boundaryml.com'>Docs</VSCodeLink>
            <ShowSettingsButton buttonClassName='h-10 w-10 bg-transparent p-1' iconClassName='h-7 w-7' />
          </div>
          <SettingsDialog />
          <div className='flex flex-col w-full gap-2 px-2 pb-4'>
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
