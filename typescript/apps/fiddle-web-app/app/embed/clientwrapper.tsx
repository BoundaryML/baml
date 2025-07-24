'use client';
import { filesAtom } from '@baml/playground-common';
import { ResizableHandle, ResizablePanelGroup } from '@baml/ui/resizable';
import { ResizablePanel } from '@baml/ui/resizable';
import { ScrollArea } from '@baml/ui/scroll-area';
import { useAtom, useAtomValue } from 'jotai';
import dynamic from 'next/dynamic';
import { isMobile } from 'react-device-detect';

import { Suspense, useEffect, useState } from 'react';
import { activeFileNameAtom } from '../[project_id]/_atoms/atoms';
import { ErrorBoundary } from 'react-error-boundary';
import { Button } from '@baml/ui/button';
import { RefreshCcw } from 'lucide-react';

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

// Placeholder components
const JotaiProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  return <>{children}</>;
};

const CodeMirrorViewer: React.FC<any> = ({ fileContent, onContentChange }) => {
  return (
    <textarea
      value={fileContent?.code || ''}
      onChange={(e) => onContentChange(e.target.value)}
      className="w-full h-full p-4 font-mono text-sm bg-transparent border-none outline-none resize-none"
    />
  );
};

const PromptPreview: React.FC = () => {
  return (
    <div className="flex items-center justify-center w-full h-full">
      <p className="text-muted-foreground">Prompt preview coming soon...</p>
    </div>
  );
};

const EventListener: React.FC = () => {
  return null;
};


interface EmbedComponentProps {
  bamlContent: string;
}

export default function EmbedComponent({ bamlContent }: EmbedComponentProps) {
  const [files, setFiles] = useAtom(filesAtom);
  const [isLoading, setIsLoading] = useState(true);
  const activeFileName = useAtomValue(activeFileNameAtom);

  useEffect(() => {
    // Set the files with the BAML content passed from the server
    setFiles({
      'main.baml': bamlContent,
    });
    setIsLoading(false);
  }, [bamlContent, setFiles]);

  if (isLoading) {
    return <div className="text-white">Loading BAML file...</div>;
  }

  return (
    <div className="flex justify-center items-center w-screen h-screen bg-background">
      <EventListener />
      {/* <h1 className='text-xl font-bold text-gray-500'>This is an embeddable React Component!</h1> */}
      {/* <p className='text-gray-600'>You can use this inside an iframe.</p> */}
      <ResizablePanelGroup
        className="min-h-[200px] w-full rounded-lg overflow-clip"
        direction="horizontal"
      >
        <ResizablePanel defaultSize={50}>
          <div className="flex pl-1 w-full h-full tour-editor dark:bg-muted/70">
            <ScrollArea className="w-full h-full">
              {activeFileName && (
                <CodeMirrorViewer
                  lang="baml"
                  fileContent={{
                    code: files[activeFileName] || '',
                    language: 'baml',
                    id: activeFileName,
                  }}
                  hideLineNumbers={true}
                  shouldScrollDown={false}
                  onContentChange={(v: string) => {
                    const newFiles: Record<string, string> = {};
                    Object.entries(files).map(([key, value]) => {
                      const newVal = key === activeFileName ? v : value;
                      newFiles[key] = newVal;
                    });
                    setFiles(newFiles);
                  }}
                />
              )}
            </ScrollArea>
          </div>
        </ResizablePanel>
        <ResizableHandle className="" />
        {!isMobile && (
          <ResizablePanel defaultSize={50} className="tour-playground">
            <div className="flex flex-row h-full">
              <PlaygroundView />
            </div>
          </ResizablePanel>
        )}
      </ResizablePanelGroup>
    </div>
  );
}

const PlaygroundView = () => {
  return (
    <>
      <CustomErrorBoundary message="Error loading playground">
        <Suspense fallback={<div>Loading...</div>}>
          <div className="flex flex-col w-full h-full">
            <PromptPreview />
          </div>

          {/* <InitialTour /> */}
          {/* <PostTestRunTour /> */}
        </Suspense>
      </CustomErrorBoundary>
    </>
  );
};
