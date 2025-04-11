import { Suspense } from 'react'
import './App.css'
import 'allotment/dist/style.css'
import { DevTools } from 'jotai-devtools'
import { FlaskConical, FlaskConicalOff, Compass } from 'lucide-react'
import { EventListener } from './baml_wasm_web/EventListener'
import { Button } from './components/ui/button'
import { Separator } from './components/ui/separator'
// import FunctionPanel from './shared/FunctionPanel'
// import { ViewSelector } from './shared/Selectors'
// import SettingsDialog, { ShowSettingsButton, showSettingsAtom } from './shared/SettingsDialog'
// import IntroToChecksDialog from './shared/IntroToChecksDialog'
import CustomErrorBoundary from './utils/ErrorFallback'
import 'jotai-devtools/styles.css'
// import { Snippets } from './shared/Snippets'
import { Dialog, DialogTrigger, DialogContent } from './components/ui/dialog'
// import { AppStateProvider } from './shared/AppStateContext' // Import the AppStateProvider
import { useFeedbackWidget } from './lib/feedback_widget'
import PromptPreview from './shared/baml-project-panel/playground-panel/prompt-preview'
import { ThemeProvider } from 'next-themes'

function App() {
  useFeedbackWidget()
  return (
    <CustomErrorBoundary message='Error loading playground'>
      <DevTools />
      <Suspense fallback={<div>Loading...</div>}>
        <EventListener>
          <div></div>
        </EventListener>
        {/* <div>hi</div> */}
        <div className='h-[100vh]'>
          <ThemeProvider attribute='class' defaultTheme='dark' enableSystem={false} disableTransitionOnChange={true}>
            <PromptPreview />
          </ThemeProvider>
        </div>

        {/* <AppStateProvider>
            <div className='flex flex-col w-full gap-2 px-2 pb-1 h-[100vh] overflow-y-clip'>
              <div className='flex flex-row gap-1 justify-start items-center'>
                <CustomErrorBoundary message='Error loading view selector'>
                  <ViewSelector />
                </CustomErrorBoundary>
              </div>
              <Separator className='bg-vscode-textSeparator-foreground' />
              <FunctionPanel />
            </div>
            <CustomErrorBoundary message='Error loading settings dialog'>
              <SettingsDialog />
            </CustomErrorBoundary>
            <CustomErrorBoundary message='Error loading intro to checks dialog'>
              <IntroToChecksDialog />
            </CustomErrorBoundary>
          </AppStateProvider>{' '} */}
      </Suspense>
    </CustomErrorBoundary>
  )
}

export default App
