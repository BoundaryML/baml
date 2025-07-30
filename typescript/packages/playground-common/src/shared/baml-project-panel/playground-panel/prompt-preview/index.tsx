'use client';
import { SidebarInset, SidebarProvider } from '@baml/ui/sidebar';
import { useAtomValue } from 'jotai';
import { ApiKeysDialog } from '../../../../components/api-keys-dialog/dialog';
import { StatusBar } from '../../../../components/status-bar';
import { wasmAtom } from '../../atoms';
import { PreviewToolbar } from '../preview-toolbar';
import { TestingSidebar } from '../side-bar';
import { Loader } from './components';
import { PromptRenderWrapper } from './prompt-render-wrapper';
import { TestPanel } from './test-panel';

export const PromptPreview = () => {
  const wasm = useAtomValue(wasmAtom);

  return (
    <SidebarProvider defaultOpen={true}>
      <SidebarInset className="h-screen flex flex-col overflow-hidden relative">
        {wasm ? (
          <>
            {/* Header - always at top */}
            <div className="flex-shrink-0 px-4 py-2 min-w-0 overflow-hidden">
              <PreviewToolbar />
            </div>

            {/* Scrollable Body - takes remaining space */}
            <div className="flex-1 overflow-y-auto min-h-0 pb-14 px-4">
              <PromptRenderWrapper />
              <TestPanel />
            </div>

            {/* Footer - always at bottom */}
            <div className="flex-shrink-0 absolute bottom-0 left-0 right-0 flex">
              <StatusBar />
            </div>
          </>
        ) : (
          <Loader message="Loading..." />
        )}
      </SidebarInset>
      <TestingSidebar />
      <ApiKeysDialog />
    </SidebarProvider>
  );
};
