import { Suspense } from 'react';
import 'allotment/dist/style.css';
// import { Snippets } from './shared/Snippets'
// import { AppStateProvider } from './shared/AppStateContext' // Import the AppStateProvider
import { useFeedbackWidget } from '@baml/playground-common';
// import FunctionPanel from './shared/FunctionPanel'
// import { ViewSelector } from './shared/Selectors'
// import SettingsDialog, { ShowSettingsButton, showSettingsAtom } from './shared/SettingsDialog'
// import IntroToChecksDialog from './shared/IntroToChecksDialog'
import { CustomErrorBoundary } from '@baml/playground-common/custom-error-boundary';
import { EventListener } from '@baml/playground-common/event-listener';
// import 'jotai-devtools/styles.css'
import { PromptPreview } from '@baml/playground-common/prompt-preview';
import { ThemeProvider } from 'next-themes';
import { useWasmPanicHandler } from '@baml/playground-common/baml-project-panel/atoms';
import { WasmPanicNotification } from '@baml/playground-common/baml-project-panel/WasmPanicNotification';

function App() {
  useFeedbackWidget();
  // Wire up WASM panic handler to automatically cancel tests on panic
  useWasmPanicHandler();

  return (
    <CustomErrorBoundary message="Error loading playground">
      {/* <DevTools /> */}
      <Suspense fallback={<div>Loading...</div>}>
        <div className="h-screen bg-background text-foreground">
          <ThemeProvider
            attribute="class"
            defaultTheme="dark"
            enableSystem
            disableTransitionOnChange={true}
          >
            {/* WASM panic notification */}
            <WasmPanicNotification />

            {/* Main content area */}
            <div className="h-full">
              <PromptPreview />
            </div>

            {/* Background event handler (no UI) */}
            <EventListener />
          </ThemeProvider>
        </div>

        {/* <AppStateProvider>
            <div className='flex flex-col w-full gap-2 px-2 pb-1 h-screen overflow-y-clip'>
              <div className='flex flex-row gap-1 justify-start items-center'>
                <CustomErrorBoundary message='Error loading view selector'>
                  <ViewSelector />
                </CustomErrorBoundary>
              </div>
              <Separator className='bg-vscode-text-separator-foreground' />
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
  );
}

export default App;
