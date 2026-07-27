'use client';
import { atom, createStore, useAtom, useAtomValue } from 'jotai';
import { useCallback, useEffect, useMemo } from 'react';
import { ErrorBoundary } from 'react-error-boundary';
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@/components/ui/resizable';
import { ScrollArea } from '@/components/ui/scroll-area';

import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import JotaiProvider from './baml-project-panel/Jotai';
import {
  filesAtom,
  generatedFilesByLangAtom,
  sandboxFilesAtom,
} from './baml-project-panel/atoms';
import SandboxPanel from './baml-project-panel/code-sandbox/SandboxPanel';
import { CodeMirrorViewer } from './baml-project-panel/codemirror-panel/code-mirror-viewer';
import PromptPreview from './baml-project-panel/playground-panel/prompt-preview';
import { type ICodeBlock } from './baml-project-panel/types';

const fakePythonCode = `
def hello_world():
    print("Hello, world!")
`;

const blockIdAtom = atom<string>('');

const fakeBamlCode = `
class Hi {
  prop string @description("This is a stringggg")
}

`;

const fakePythonCodeBlock: ICodeBlock = {
  code: fakePythonCode,
  language: 'python',
  id: 'main.py',
};

const fakeTypescriptCode = `
import { b } from "./baml_client";
let hasAnError: string = 10;

function increment(num: number) {
  return num + 1;
}

interface Hello {
  name: string;
}

const hello: Hello = {
  name: "Aaron",
};

console.log(hello.name);

increment('not a number');
console.log("Hello, world! from TS");

`;
const fakeTypescriptCodeBlock: ICodeBlock = {
  code: fakeTypescriptCode,
  language: 'typescript',
  id: 'main.ts',
};

export default function BamlBlock({
  name,
  bamlCode,
  languageCode,
  language,
}: {
  name: string;
  bamlCode: string;
  languageCode: string;
  language: 'python' | 'typescript';
}) {
  const store = useMemo(() => {
    const uniqueStore = createStore();
    uniqueStore.set(blockIdAtom, name);
    return uniqueStore;
  }, [name]);

  return (
    <div className="not-prose h-fit w-full pb-8">
      <ErrorBoundary fallback={<div>Error rendering playground</div>}>
        <JotaiProvider store={store}>
          <BamlContainer
            bamlCode={bamlCode}
            id={name}
            languageCode={languageCode}
            language={language}
          />
        </JotaiProvider>
      </ErrorBoundary>
    </div>
  );
}

const BamlContainer = ({
  bamlCode,
  id,
  languageCode,
  language,
}: // status,
{
  bamlCode: string;
  id: string;
  languageCode: string | undefined;
  language: 'python' | 'typescript';
  // status: "loading" | "success" | "error";
}) => {
  const [file, setFile] = useAtom(filesAtom);
  const [sandboxFile, setSandboxFile] = useAtom(sandboxFilesAtom);
  const generated = useAtomValue(generatedFilesByLangAtom(language));

  const handleContentChange = useCallback(
    (content: string) => {
      setFile(content);
    },
    [setFile],
  );

  useEffect(() => {
    setFile(bamlCode);
  }, [bamlCode, setFile]);

  useEffect(() => {
    setSandboxFile(languageCode || '');
  }, [languageCode, setSandboxFile]);

  return (
    <>
      <Tabs defaultValue="baml" className="h-[600px] w-full">
        <TabsList className="flex border-b">
          <TabsTrigger value="baml" className="flex-1">
            Prompt with BAML{' '}
            {/* {status === "loading" && (
              <Loader2 className="ml-2 inline h-4 w-4 animate-spin" />
            )} */}
          </TabsTrigger>
          {language !== undefined && (
            <TabsTrigger value="run" className="flex-1">
              Run it in {language}{' '}
              {/* {status === "loading" && (
              <Loader2 className="ml-2 inline h-4 w-4 animate-spin" />
            )} */}
            </TabsTrigger>
          )}
        </TabsList>
        <TabsContent value="baml" className="h-[calc(100%-70px)] w-full">
          <ResizablePanelGroup
            direction="horizontal"
            className="h-full overflow-y-clip"
          >
            <ResizablePanel defaultSize={40} className="h-full">
              <ScrollArea className="relative h-full flex-1" type="always">
                <ErrorBoundary fallback={<div>Error</div>}>
                  <CodeMirrorViewer
                    lang="baml"
                    generatedFiles={[]}
                    fileContent={{
                      code: file,
                      language: 'baml',
                      id,
                    }}
                    onContentChange={handleContentChange}
                    shouldScrollDown={false}
                    isReadOnly={false}
                  />
                  {/* {status === "loading" && (
                    <div className="absolute right-4 top-4 flex items-center justify-center text-gray-500">
                      <Loader2 className="h-4 w-4 animate-spin" />
                    </div>
                  )} */}
                </ErrorBoundary>
              </ScrollArea>
            </ResizablePanel>
            <ResizableHandle withHandle className="border-none !bg-inherit" />
            <ResizablePanel
              defaultSize={60}
              className="h-full border-[1px] border-gray-200"
            >
              <PromptPreview />
            </ResizablePanel>
          </ResizablePanelGroup>
        </TabsContent>
        <TabsContent value="run" className="h-full w-full ">
          <div className="flex h-full flex-col">
            <SandboxPanel
              codeBlock={{
                language: language,
                code: sandboxFile,
                id: `${id}-sandbox`,
              }}
              isCodeblockLoading={false}
              generated={generated ?? []}
            />
          </div>
        </TabsContent>
      </Tabs>
    </>
  );
};
