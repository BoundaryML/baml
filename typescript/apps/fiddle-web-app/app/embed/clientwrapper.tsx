'use client';
import { filesAtom } from '@baml/playground-common';
import { PromptPreview } from '@baml/playground-common/prompt-preview';
import { BAMLSDKProvider, useBAMLSDK } from '@baml/playground-common/sdk';
import { CodeMirrorViewer } from '@baml/playground-common/codemirror-viewer';
import { EventListener } from '@baml/playground-common/event-listener';
import { ResizableHandle, ResizablePanelGroup } from '@baml/ui/resizable';
import { ResizablePanel } from '@baml/ui/resizable';
import { useAtomValue, useSetAtom } from 'jotai';
import { isMobile } from 'react-device-detect';

import { Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { activeFileNameAtom } from '../[project_id]/_atoms/atoms';
import { ErrorBoundary } from 'react-error-boundary';
import { Button } from '@baml/ui/button';
import { RefreshCcw } from 'lucide-react';
import FileViewer from '../[project_id]/_components/Tree/FileViewer';
import { useSearchParams } from 'next/navigation';

// Custom Error Boundary component
const CustomErrorBoundary: React.FC<{ children: React.ReactNode; message?: string }> = ({
  children,
  message
}) => {
  return (
    <ErrorBoundary
      fallbackRender={({ error, resetErrorBoundary }) => (
        <div
          role="alert"
          className="p-4 rounded border bg-vscode-notifications-background border-vscode-notifications-border"
        >
          <div className="flex justify-between items-center mb-4">
            <p className="font-medium text-vscode-foreground">
              {message ?? 'Something went wrong'}
            </p>
            <Button
              onClick={resetErrorBoundary}
              variant="outline"
              className="hover:bg-vscode-button-hover-background"
            >
              <RefreshCcw className="w-4 h-4" />
              Reload
            </Button>
          </div>
          {error instanceof Error && (
            <div className="space-y-2">
              <pre className="p-3 text-sm whitespace-pre-wrap rounded border bg-vscode-editor-background border-vscode-panel-border">
                {error.message}
              </pre>
            </div>
          )}
        </div>
      )}
      onReset={() => {
        if (typeof window === 'undefined') {
          return;
        }
        window.location.reload();
      }}
    >
      {children}
    </ErrorBoundary>
  );
};


type EditorFile = { path: string; content: string };

interface EmbedComponentProps {
  files: EditorFile[];
  // All UI toggles and optional default file are taken from URL via useSearchParams
}

export default function EmbedComponent({ files }: EmbedComponentProps) {
  // Convert files array to record format for SDK
  const initialFiles = useMemo(() => {
    const record: Record<string, string> = {};
    for (const f of files) {
      record[f.path] = f.content;
    }
    return record;
  }, [files]);

  return (
    <BAMLSDKProvider mode="wasm" initialFiles={initialFiles}>
      <EmbedComponentInner files={files} />
    </BAMLSDKProvider>
  );
}

function EmbedComponentInner({ files }: EmbedComponentProps) {
  const sdk = useBAMLSDK();
  const editorFiles = useAtomValue(filesAtom);
  const [previewReady, setPreviewReady] = useState(false);
  // SDK provider already ensures WASM is loaded before rendering children
  const activeFileName = useAtomValue(activeFileNameAtom);
  const setActiveFileName = useSetAtom(activeFileNameAtom);
  const searchParams = useSearchParams();
  const uiToggles = useMemo(() => {
    const getBool = (key: string, defaultValue: boolean) => {
      const val = searchParams.get(key);
      if (val === null) return defaultValue;
      return val === 'true';
    };
    return {
      showFileTree: getBool('showFileTree', false),
      showFile: getBool('showFile', true),
      showPlayground: getBool('showPlayground', true),
    };
  }, [searchParams]);

  // Note: Initial files are provided via BAMLSDKProvider's initialFiles prop
  // No need for a useEffect here - the provider handles initialization

  // Apply default file if provided via URL (takes precedence once on mount when valid)
  const appliedDefaultRef = useRef(false);
  useEffect(() => {
    if (appliedDefaultRef.current) return;
    const availablePaths = Object.keys(editorFiles);
    const isValidPath = (p: string | null | undefined) => !!p && availablePaths.includes(p);
    const defaultFile = searchParams.get('defaultFile') ?? undefined;
    if (isValidPath(defaultFile)) {
      appliedDefaultRef.current = true;
      setActiveFileName(defaultFile as string);
    }
  }, [editorFiles, setActiveFileName, searchParams]);

  // Mark preview as ready after first paint to avoid flash between loader and preview
  useEffect(() => {
    if (!previewReady) {
      const id = requestAnimationFrame(() => setPreviewReady(true));
      return () => cancelAnimationFrame(id);
    }
  }, [previewReady]);

  return (
    <div className="flex justify-center items-center w-screen h-screen bg-background relative">
      <div className="absolute bottom-0 right-4 z-50">
        <EventListener />
      </div>
      {/* <h1 className='text-xl font-bold text-gray-500'>This is an embeddable React Component!</h1> */}
      {/* <p className='text-gray-600'>You can use this inside an iframe.</p> */}
      <div className="flex w-full h-full">
        {uiToggles.showFileTree && (
          <div className="w-64 h-full dark:bg-[#020309] bg-muted">
            <div className="flex flex-col pb-2 w-full h-full">
              <FileViewer />
            </div>
          </div>
        )}

        <div className="flex-1 flex flex-col w-full h-full">
          <ResizablePanelGroup
            className="min-h-[200px] w-full rounded-lg overflow-clip"
            direction="horizontal"
          >
            {uiToggles.showFile && (
              <ResizablePanel defaultSize={uiToggles.showPlayground && !isMobile ? 50 : 100}>
                <div className="flex pl-1 w-full h-full tour-editor dark:bg-muted/70 overflow-y-auto">
                  {activeFileName && (
                    <CodeMirrorViewer
                      lang="baml"
                      fileContent={{
                        code: editorFiles[activeFileName] || '',
                        language: 'baml',
                        id: activeFileName,
                      }}
                      hideLineNumbers={true}
                      shouldScrollDown={false}
                      onContentChange={(v: string) => {
                        const newFiles: Record<string, string> = {};
                        Object.entries(editorFiles).forEach(([key, value]) => {
                          newFiles[key] = key === activeFileName ? v : value;
                        });
                        sdk.files.update(newFiles);
                      }}
                    />
                  )}
                </div>
              </ResizablePanel>
            )}
            {uiToggles.showFile && uiToggles.showPlayground && !isMobile && <ResizableHandle className="" />}
            {uiToggles.showPlayground && !isMobile && (
              <ResizablePanel defaultSize={uiToggles.showFile ? 50 : 100} className="tour-playground">
                <div className="flex flex-row h-full">
                  <PlaygroundView onReady={() => { /* handled by RAF hook */ }} />
                </div>
              </ResizablePanel>
            )}
          </ResizablePanelGroup>
        </div>
      </div>
    </div>
  );
}

const PlaygroundView = ({ onReady }: { onReady: () => void }) => {
  return (
    <>
      <CustomErrorBoundary message="Error loading playground">
        <Suspense fallback={null}>
          <div className="flex flex-col w-full h-full">
            <OnMount onReady={onReady}>
              <PromptPreview />
            </OnMount>
          </div>

          {/* <InitialTour /> */}
          {/* <PostTestRunTour /> */}
        </Suspense>
      </CustomErrorBoundary>
    </>
  );
};

const OnMount = ({ onReady, children }: { onReady: () => void; children: React.ReactNode }) => {
  useEffect(() => {
    onReady();
  }, [onReady]);
  return <>{children}</>;
};

// Using shared BrandedLoading and InlineLoading components
