'use client';

import { CodeMirrorViewer } from '@baml/playground-common/codemirror-viewer';
import { CustomErrorBoundary } from '@baml/playground-common/custom-error-boundary';
import { EventListener } from '@baml/playground-common/event-listener';
import { BAMLSDKProvider } from '@baml/playground-common/sdk';
import { PromptPreview } from '@baml/playground-common/prompt-preview';
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@baml/ui/resizable';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { Suspense, useEffect, useRef, useState } from 'react';
import { isMobile } from 'react-device-detect';
import { useKeybindingOverrides } from '../../../hooks/command-s';
import type { BAMLProject } from '../../../lib/exampleProjects';
import { Editable } from '../../_components/EditableText';
import { activeFileNameAtom, unsavedChangesAtom } from '../_atoms/atoms';

import {
  filesAtom,
  functionsAtom,
  unifiedSelectionStateAtom,
  type SelectionState,
} from '@baml/playground-common';
import { useBAMLSDK } from '@baml/playground-common/sdk';
import { useFeedbackWidget } from '@baml/playground-common/lib/feedback_widget';
import { ScrollArea } from '@baml/ui/scroll-area';
import Image from 'next/image';
import { TopNavbar } from './TopNavbar';
import FileViewer from './Tree/FileViewer';

const ErrorBoundaryWrapper = ({
  message,
  children,
}: {
  message: string;
  children: React.ReactNode;
}) => <CustomErrorBoundary message={message}>{children}</CustomErrorBoundary>;

// Hook for project file management
const useProjectFiles = (project: BAMLProject) => {
  const sdk = useBAMLSDK();
  const files = useAtomValue(filesAtom);
  const [unsavedChanges, setUnsavedChanges] = useAtom(unsavedChangesAtom);

  useEffect(() => {
    if (project) {
      console.log('Updating files via SDK: project', project.id);
      setUnsavedChanges(false);
      const newFiles = project.files.reduce(
        (acc, f) => {
          acc[f.path] = f.content;
          return acc;
        },
        {} as Record<string, string>,
      );
      // Use SDK to update files and recreate runtime
      sdk.files.update(newFiles);
    }
  }, [project, sdk, setUnsavedChanges]);

  // Wrapper to update files via SDK
  const setFiles = (newFiles: Record<string, string>) => {
    sdk.files.update(newFiles);
  };

  return { files, setFiles, unsavedChanges };
};

// Hook for editable text fields
const useEditableField = (initialValue: string) => {
  const [value, setValue] = useState(initialValue);
  const editableRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  return { value, setValue, editableRef, textareaRef };
};

const ProjectViewImpl = ({ project }: { project: BAMLProject }) => {
  useFeedbackWidget();
  useKeybindingOverrides();

  const { files, setFiles, unsavedChanges } = useProjectFiles(project);
  const activeFileName = useAtomValue(activeFileNameAtom);
  const { value: projectName, setValue: setProjectName } = useEditableField(
    project.name,
  );
  const projectNameInputRef = useRef<HTMLInputElement>(null);
  const {
    value: description,
    setValue: setDescription,
    editableRef: descriptionInputRef,
    textareaRef: descriptionTextareaRef,
  } = useEditableField(project.description);

  const handleContentChange = (newContent: string) => {
    const newFiles: Record<string, string> = {};
    for (const [key, value] of Object.entries(files)) {
      newFiles[key] = key === activeFileName ? newContent : value;
    }
    setFiles(newFiles);
  };

  return (
    <div className="flex relative flex-col w-full h-full main-panel overflow-x-hidden overflow-y-hidden">
      <ErrorBoundaryWrapper message="Error loading project">
        <div className="absolute bottom-0 right-4 z-50">
          <EventListener />
        </div>

        {isMobile && (
          <div className="absolute bottom-0 left-0 right-0 font-semibold border-t-[1px] w-full h-[100px] z-50 text-center p-8">
            Visit PromptFiddle on Desktop to get the best experience
          </div>
        )}

        <div className="flex w-full h-full">
          {!isMobile && <ProjectSidebar />}

          <div className="flex-1 flex flex-col w-full h-full font-sans">
            <TopNavbar
              project={project}
              projectName={projectName}
              setProjectName={setProjectName}
              projectNameInputRef={projectNameInputRef}
              unsavedChanges={unsavedChanges}
            />

            <div
              style={{ height: 'calc(100% - 55px)' }}
              className="flex flex-row h-full overflow-hidden"
            >
              <ResizablePanelGroup
                className="min-h-[200px] w-full rounded-lg overflow-clip"
                direction="horizontal"
              >
                <ResizablePanel defaultSize={50}>
                  <div className="flex flex-col py-1 pl-2 w-full text-xs whitespace-nowrap border-none items-left h-fit">
                    <Editable
                      text={description}
                      placeholder="Write a task name"
                      type="input"
                      childRef={descriptionInputRef}
                      className="px-2 py-2 w-full text-sm font-normal text-left border-none text-foreground"
                    >
                      <textarea
                        className="w-[95%] ml-2 px-2 text-sm border-none"
                        ref={descriptionTextareaRef}
                        name="task"
                        placeholder="Write a description"
                        value={description}
                        onChange={(e) => setDescription(e.target.value)}
                      />
                    </Editable>
                  </div>

                  <div className="flex pl-1 w-full h-full tour-editor dark:bg-muted/70 overflow-hidden">
                    <ScrollArea className="w-full h-full">
                      {activeFileName && (
                        <CodeMirrorViewer
                          lang="baml"
                          fileContent={{
                            code: files[activeFileName] || '',
                            language: 'baml',
                            id: activeFileName,
                          }}
                          shouldScrollDown={false}
                          onContentChange={handleContentChange}
                        />
                      )}
                    </ScrollArea>
                  </div>
                </ResizablePanel>

                <ResizableHandle />

                {!isMobile && (
                  <ResizablePanel defaultSize={50} className="tour-playground">
                    <div className="flex flex-col h-full overflow-hidden">
                      <div className="h-full min-h-0 overflow-hidden">
                        <PlaygroundView />
                      </div>
                    </div>
                  </ResizablePanel>
                )}
              </ResizablePanelGroup>
            </div>
          </div>
        </div>

        <FunctionSelectorProvider />
      </ErrorBoundaryWrapper>
    </div>
  );
};

export const FunctionSelectorProvider = () => {
  const activeFileName = useAtomValue(activeFileNameAtom);
  const functions = useAtomValue(functionsAtom);
  const setSelectionState = useSetAtom(unifiedSelectionStateAtom);
  const currentSelection = useAtomValue(unifiedSelectionStateAtom);
  const hasInitializedRef = useRef(false);

  useEffect(() => {
    // Find the first function in the active file
    const func = functions.find((f) => f.span?.filePath === activeFileName);

    if (func) {
      // Get the first test case if available
      const firstTestName = func.testCases?.[0]?.name ?? null;

      // Only auto-select on initial load or when file changes
      // Check if we already have this function selected to avoid loops
      if (currentSelection.mode === 'function' &&
          currentSelection.functionName === func.name) {
        // Already selected this function, only update test if empty
        if (!currentSelection.testName && firstTestName) {
          setSelectionState({
            mode: 'function',
            functionName: func.name,
            testName: firstTestName,
          });
        }
        return;
      }

      // Select the function and its first test
      setSelectionState({
        mode: 'function',
        functionName: func.name,
        testName: firstTestName,
      });
      hasInitializedRef.current = true;
    } else if (functions.length > 0 && !hasInitializedRef.current) {
      // No function in active file, but we have functions - select the first one
      const firstFunc = functions[0];
      if (firstFunc) {
        const firstTestName = firstFunc.testCases?.[0]?.name ?? null;
        setSelectionState({
          mode: 'function',
          functionName: firstFunc.name,
          testName: firstTestName,
        });
        hasInitializedRef.current = true;
      }
    }
  }, [activeFileName, functions, setSelectionState, currentSelection]);

  return null;
};

export const ProjectSidebar = () => (
  <div className="w-[200px] h-full dark:bg-[#020309] bg-muted overflow-hidden">
    <div className="flex flex-row justify-center items-center pt-4 w-full">
      <a
        href={'/'}
        className="flex flex-row items-center text-lg font-semibold text-center w-fit text-foreground"
      >
        <Image
          src="/baml-lamb-white.png"
          alt="Prompt Fiddle"
          width={40}
          height={40}
        />
        Prompt Fiddle
      </a>
    </div>

    <div className="pb-4 h-full overflow-hidden">
      <div className="px-2 pt-4 w-full text-xs font-normal text-center uppercase text-muted-foreground">
        project files
      </div>
      <div className="flex flex-col pb-8 w-full h-full min-h-0 tour-file-view">
        <div className="flex-1 min-h-0 overflow-y-auto">
          <FileViewer />
        </div>
      </div>
    </div>
  </div>
);

export const ProjectView = ({ project }: { project: BAMLProject }) => (
  <BAMLSDKProvider mode="wasm">
    <ProjectViewImpl project={project} />
  </BAMLSDKProvider>
);

const PlaygroundView = () => (
  <ErrorBoundaryWrapper message="Error loading playground">
    <Suspense fallback={<div>Loading...</div>}>
      <PromptPreview />
    </Suspense>
  </ErrorBoundaryWrapper>
);

export default ProjectView;
