import { Suspense } from 'react';
import 'allotment/dist/style.css';
import { EventListener } from '@baml/playground-common/event-listener';
// import FunctionPanel from './shared/FunctionPanel'
// import { ViewSelector } from './shared/Selectors'
// import SettingsDialog, { ShowSettingsButton, showSettingsAtom } from './shared/SettingsDialog'
// import IntroToChecksDialog from './shared/IntroToChecksDialog'
import { CustomErrorBoundary } from '@baml/playground-common/custom-error-boundary';
// import 'jotai-devtools/styles.css'
import { PromptPreview } from '@baml/playground-common/prompt-preview';
// import { Snippets } from './shared/Snippets'
// import { AppStateProvider } from './shared/AppStateContext' // Import the AppStateProvider
import { useFeedbackWidget } from '@baml/playground-common';
import { ThemeProvider } from 'next-themes';

function App() {
  useFeedbackWidget();
  return (
    <CustomErrorBoundary message="Error loading playground">
      {/* <DevTools /> */}
      <Suspense fallback={<div>Loading...</div>}>

        <div className="relative min-h-screen bg-background text-foreground p-2">
          <ThemeProvider
            attribute="class"
            defaultTheme="dark"
            enableSystem
            disableTransitionOnChange={true}
          >
            <PromptPreview />
            <div className="absolute bottom-0 right-4 z-50">
              <EventListener />
            </div>
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
