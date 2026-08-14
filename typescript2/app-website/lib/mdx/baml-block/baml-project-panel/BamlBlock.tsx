'use client';
import { atom, createStore, useAtomValue, useSetAtom } from 'jotai';
import { useCallback, useEffect, useMemo } from 'react';
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@/components/ui/resizable';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import JotaiProvider from './Jotai';
import { filesAtom, generatedFilesByLangAtom } from './atoms';
import SandboxPanel from './code-sandbox/SandboxPanel';

import { CodeMirrorViewer } from './codemirror-panel/code-mirror-viewer';
import PromptPreview from './playground-panel/prompt-preview';
import { type ICodeBlock } from './types';

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
  language: ICodeBlock['language'];
}) {
  const store = useMemo(() => {
    const uniqueStore = createStore();
    uniqueStore.set(blockIdAtom, name);
    return uniqueStore;
  }, [name]);

  return (
    <div className="not-prose h-fit w-full pb-24">
      <JotaiProvider store={store}>
        <BamlContainer
          bamlCode={bamlCode}
          id={name}
          languageCode={languageCode}
          language={language}
        />
      </JotaiProvider>
    </div>
  );
}

const BamlContainer = ({
  bamlCode,
  id,
  languageCode,
  language,
}: {
  bamlCode: string;
  id: string;
  languageCode: string;
  language: ICodeBlock['language'];
}) => {
  const setFile = useSetAtom(filesAtom);

  const handleContentChange = useCallback((content: string) => {
    setFile(content);
  }, []);

  const generated = useAtomValue(generatedFilesByLangAtom(language));
  const fileContent = {
    code: bamlCode ?? fakeBamlCode,
    // code: "",
    language,
    id,
  };

  useEffect(() => {
    setFile(bamlCode);
  }, [bamlCode]);

  return (
    <>
      <ResizablePanelGroup
        direction="horizontal"
        className="max-h-[600px] overflow-y-clip"
      >
        <ResizablePanel defaultSize={40} className="max-h-[600px]">
          <Tabs defaultValue="main.baml" className="w-full border-b">
            <TabsList className="w-fit rounded-none border-b bg-transparent">
              <TabsTrigger
                value="main.baml"
                className="hover:text-blue-500: relative mb-0 !rounded-none border border-b-4 border-l-0  border-r-0 border-b-blue-400 !bg-inherit  px-3 py-2 text-primary !shadow-none transition-colors data-[state=active]:text-blue-500"
              >
                main.baml
              </TabsTrigger>
            </TabsList>
          </Tabs>
          <div className="">
            <ScrollArea className="h-[400px]" type="always">
              <CodeMirrorViewer
                lang="baml"
                generatedFiles={[]}
                fileContent={fileContent}
                onContentChange={handleContentChange}
                shouldScrollDown={false}
              />
            </ScrollArea>
          </div>
        </ResizablePanel>
        <ResizableHandle withHandle className="border-none !bg-inherit" />
        <ResizablePanel
          defaultSize={60}
          className="rounded-lg border-[1px] border-gray-200 pt-0 shadow-sm"
        >
          <PromptPreview />
        </ResizablePanel>
      </ResizablePanelGroup>
      <div className="mt-4">{language} usage</div>
      <SandboxPanel
        isCodeblockLoading={false}
        codeBlock={{
          language,
          code: languageCode,
          id,
        }}
        generated={generated ?? []}
      />
    </>
  );
};
