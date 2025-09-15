'use client';
import { CopyButton } from '@baml/ui/custom/copy-button';
import { SidebarInset, SidebarProvider } from '@baml/ui/sidebar';
import { useAtomValue } from 'jotai';
import { ApiKeysDialog } from '../../../../components/api-keys-dialog/dialog';
import { StatusBar } from '../../../../components/status-bar';
import { wasmAtom } from '../../atoms';
import { vscode } from '../../vscode';
import { functionTestSnippetAtom, selectionAtom } from '../atoms';
import { PreviewToolbar } from '../preview-toolbar';
import { TestingSidebar } from '../side-bar';
import { Loader } from './components';
import { PromptRenderWrapper } from './prompt-render-wrapper';
import { TestPanel } from './test-panel';

export const NoTestsContent = () => {
  const { selectedFn } = useAtomValue(selectionAtom);
  const testSnippet = useAtomValue(
    functionTestSnippetAtom(selectedFn?.name ?? ''),
  );

  // Check if the function has any valid test cases
  const hasValidTestCases =
    selectedFn?.test_cases && selectedFn.test_cases.length > 0;

  const message = hasValidTestCases
    ? 'Add a test to see the preview!'
    : 'This function has no active test cases. Copy the template to create a test case.';

  return (
    <div className="flex flex-col gap-y-4">
      <div className="relative border-l-4 pl-2 rounded border-chart-3">
        <div className="flex w-full items-center justify-between p-3 bg-accent rounded">
          <div className="flex flex-col items-start gap-1 flex-1 overflow-hidden min-w-0 w-full">
            <div className="text-xs text-muted-foreground font-mono">
              No Test Selected
            </div>
            <div className="text-sm text-muted-foreground mt-1">{message}</div>
          </div>
        </div>
      </div>

      {testSnippet && (
        <div className="relative border-l-4 pl-2 rounded border-chart-3">
          <div className="relative">
            <pre className="rounded-md p-4 text-sm font-mono overflow-x-auto bg-accent">
              <code
                style={{ backgroundColor: 'transparent', fontSize: '12px' }}
              >
                {testSnippet}
              </code>
              <div className="flex mt-4">
                <CopyButton
                  text={testSnippet}
                  size="sm"
                  variant="outline"
                  className="flex items-center gap-2"
                  showToast={false}
                >
                  Copy Test
                </CopyButton>
              </div>
            </pre>
          </div>
        </div>
      )}
    </div>
  );
};

export const PromptPreview = () => {
  const wasm = useAtomValue(wasmAtom);
  const { selectedTc } = useAtomValue(selectionAtom);

  return (
    <>
      <SidebarProvider defaultOpen={vscode.isVscode()} className="h-full">
        <SidebarInset>
          {wasm ? (
            <div className="h-full flex flex-col overflow-hidden relative">
              {/* Header - always at top */}
              <div className="flex-shrink-0 px-4 py-2 min-w-0 overflow-hidden">
                <PreviewToolbar />
              </div>

              {/* Scrollable Body - takes remaining space */}
              <div className="flex-1 overflow-y-auto min-h-0 pb-14 px-1 min-w-0">
                {selectedTc ? (
                  <>
                    <PromptRenderWrapper />
                    <TestPanel />
                  </>
                ) : (
                  <NoTestsContent />
                )}
              </div>

              {/* Footer - always at bottom */}
              <div className="flex-shrink-0 absolute bottom-0 left-0 right-0 flex">
                <StatusBar />
              </div>
            </div>
          ) : (
            <Loader message="Loading..." />
          )}
        </SidebarInset>
        <TestingSidebar />
      </SidebarProvider>
      <ApiKeysDialog />
    </>
  );
};
