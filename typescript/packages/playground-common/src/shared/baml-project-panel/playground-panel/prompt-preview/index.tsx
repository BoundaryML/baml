'use client';
import { Loader } from '@baml/ui/custom/loader';
import { SidebarInset, SidebarProvider } from '@baml/ui/sidebar';
import { useAtomValue } from 'jotai';
import { ApiKeysDialog } from '../../../../components/api-keys-dialog/dialog';
import { wasmAtom } from '../../atoms';
import { PreviewToolbar } from '../preview-toolbar';
import { TestingSidebar } from '../side-bar';
import { PromptRenderWrapper } from './prompt-render-wrapper';
import { TestPanel } from './test-panel';

export const PromptPreview = () => {
  const wasm = useAtomValue(wasmAtom);

  return (
    <SidebarProvider defaultOpen={true}>
      <SidebarInset className="min-w-0">
        <div className="flex flex-1 flex-col gap-2 p-2 h-full min-w-0">
          {wasm ? (
            <>
              <ApiKeysDialog />
              <PreviewToolbar />
              <div className="flex-1 min-h-0 overflow-y-auto min-w-0">
                <PromptRenderWrapper />
              </div>
              <TestPanel />
            </>
          ) : (
            <Loader message="Loading..." />
          )}
        </div>
      </SidebarInset>
      <TestingSidebar />
    </SidebarProvider>
  );
};
