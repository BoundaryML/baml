import { Suspense } from 'react'
import './App.css'
import 'allotment/dist/style.css'
import { DevTools } from 'jotai-devtools'
import { FlaskConical, FlaskConicalOff, Compass } from 'lucide-react'
import { EventListener } from './baml_wasm_web/EventListener'
import { Button } from './components/ui/button'
import { Separator } from './components/ui/separator'
import FunctionPanel from './shared/FunctionPanel'
import { FunctionSelector } from './shared/Selectors'
import SettingsDialog, { ShowSettingsButton, showSettingsAtom } from './shared/SettingsDialog'
import CustomErrorBoundary from './utils/ErrorFallback'
import 'jotai-devtools/styles.css'
import { Snippets } from './shared/Snippets'
import { Dialog, DialogTrigger, DialogContent } from './components/ui/dialog'
import { CheckboxHeader } from './shared/CheckboxHeader'
import { AppStateProvider } from './shared/AppStateContext' // Import the AppStateProvider

function App() {
  return (
    <CustomErrorBoundary>
      <DevTools />
      <Suspense fallback={<div>Loading...</div>}>
        <EventListener>
          <AppStateProvider>
            {' '}
            {/* Wrap your application with AppStateProvider */}
            <div className='absolute z-10 flex flex-row items-center justify-center gap-1 right-1 top-2 text-end'>
              <Dialog>
                <DialogTrigger asChild>
                  <Button
                    variant={'ghost'}
                    className='flex flex-row items-center px-2 py-1 text-sm whitespace-pre-wrap bg-indigo-600 hover:bg-indigo-500 h-fit gap-x-2 text-vscode-button-foreground mr-2'
                  >
                    <Compass size={16} strokeWidth={2} />
                    <span className='whitespace-nowrap'>Docs</span>
                  </Button>
                </DialogTrigger>
                <DialogContent className='fullWidth min-w-full h-full border-zinc-900 bg-zinc-900'>
                  <Snippets />
                </DialogContent>
              </Dialog>
              <ShowSettingsButton buttonClassName='h-10 w-10 bg-transparent p-1' iconClassName='h-7 w-7' />
            </div>
            <SettingsDialog />
            <div className='flex flex-col w-full gap-2 px-2 pb-4'>
              <FunctionSelector />
              <Separator className='bg-vscode-textSeparator-foreground' />
              <CheckboxHeader />
              <FunctionPanel />
            </div>
          </AppStateProvider>{' '}
          {/* End of AppStateProvider */}
        </EventListener>
      </Suspense>
    </CustomErrorBoundary>
  )
}

export default App
