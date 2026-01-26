'use client';
import { CopyButton } from '@baml/ui/custom/copy-button';
import { SidebarInset, SidebarProvider } from '@baml/ui/sidebar';
import { ResizablePanelGroup, ResizablePanel, ResizableHandle } from '@baml/ui/resizable';
import { useAtom, useAtomValue } from 'jotai';
import { useEffect, useRef } from 'react';
import type { ImperativePanelHandle } from 'react-resizable-panels';
import { ApiKeysDialog } from '../../../../components/api-keys-dialog/dialog';
import { StatusBar } from '../../../../components/status-bar';
import { vscode } from '../../vscode';
import { areTestsRunningAtom, functionTestSnippetAtom, selectionAtom, viewModeAtom } from '../atoms';
import { wasmAtom } from '../../atoms';
import { PreviewToolbar } from '../preview-toolbar';
import { TestingSidebar, isSidebarOpenAtom } from '../side-bar';
import { UnifiedPromptPreview } from './unified-prompt-preview';
import { AdaptiveBottomPanel } from './adaptive-bottom-panel';
import { SelectionBridge } from '../SelectionBridge';
// disable the react-flow handle CSS
import '../../../../workflow-styles.css';

const RuntimeLoadingScreen = () => (
  <div className="h-full flex flex-col items-center justify-center gap-4 text-muted-foreground">
    <div className="h-8 w-8 border-2 border-muted-foreground border-t-transparent rounded-full animate-spin" />
    <div className="text-sm">Loading BAML runtime...</div>
  </div>
);

export const NoTestsContent = () => {
  const { selectedFn } = useAtomValue(selectionAtom);
  const testSnippet = useAtomValue(
    functionTestSnippetAtom(selectedFn?.name ?? ''),
  );

  // Check if the function has any valid test cases
  const hasValidTestCases =
    selectedFn?.testCases && selectedFn.testCases.length > 0;

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
  const { selectedTc } = useAtomValue(selectionAtom);
  const viewMode = useAtomValue(viewModeAtom);
  const [isSidebarOpen, setIsSidebarOpen] = useAtom(isSidebarOpenAtom);
  const areTestsRunning = useAtomValue(areTestsRunningAtom);
  const wasm = useAtomValue(wasmAtom);
  const bottomPanelRef = useRef<ImperativePanelHandle>(null);

  // Auto-expand bottom panel when tests start running
  useEffect(() => {
    if (bottomPanelRef.current && areTestsRunning) {
      const currentSize = bottomPanelRef.current.getSize();
      if (currentSize < 60) {
        bottomPanelRef.current.resize(60);
      }
    }
  }, [areTestsRunning]);

  // Show loading screen when WASM module hasn't loaded yet
  if (!wasm) {
    return <RuntimeLoadingScreen />;
  }

  // Check if we have content to render (tests or graph) vs showing "no tests" empty state
  const hasContentToRender = viewMode.showGraphTab || !!selectedTc;
  console.log('viewMode', viewMode);
  console.log('selectedTc', selectedTc);
  console.log('hasContentToRender', hasContentToRender);

  return (
    <>
      <SelectionBridge />
      <SidebarProvider
        open={isSidebarOpen}
        onOpenChange={setIsSidebarOpen}
        className="h-full min-h-0"
      >
        <SidebarInset>
          <div className="h-full flex flex-col overflow-hidden relative">
            {/* Header - always at top */}
            <div className="flex-shrink-0 px-4 py-2 min-w-0 overflow-hidden">
              <PreviewToolbar />
            </div>

            {/* Resizable Layout - Main Content + Bottom Panel */}
            <div className="flex-1 min-h-0">
              {hasContentToRender ? (
                <ResizablePanelGroup direction="vertical" autoSaveId="baml-playground-layout">
                  {/* Main Panel - Unified Prompt Preview with tabs */}
                  <ResizablePanel defaultSize={60} minSize={30}>
                    <div className="h-full overflow-y-auto px-1">
                      <UnifiedPromptPreview />
                    </div>
                  </ResizablePanel>

                  {/* Bottom Panel - Adaptive (TestPanel or DetailPanel) */}
                  <ResizableHandle withHandle className="cursor-row-resize after:cursor-row-resize hover:bg-blue-500 transition-all data-[panel-group-direction=vertical]:after:h-3" />
                  <ResizablePanel ref={bottomPanelRef} defaultSize={40} minSize={20} maxSize={70}>
                    <div className="h-full overflow-y-auto px-1">
                      <AdaptiveBottomPanel />
                    </div>
                  </ResizablePanel>
                </ResizablePanelGroup>
              ) : (
                <div className="overflow-y-scroll h-full px-1">
                  <NoTestsContent />
                </div>
              )}
            </div>

            {/* Footer - always at bottom */}
            <div className="flex-shrink-0 absolute bottom-0 left-0 right-0 flex">
              <StatusBar />
            </div>
          </div>
        </SidebarInset>
        <TestingSidebar />
      </SidebarProvider>
      <ApiKeysDialog />
    </>
  );
};
