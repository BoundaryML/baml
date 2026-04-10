import { useAtomValue } from 'jotai';
import { PlayCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '@/components/ui/resizable';
import { ScrollArea } from '@/components/ui/scroll-area';
import { CodeMirrorDiagnosticsAtom } from '../atoms';
import {
  CodeMirrorViewer,
  type GeneratedFile,
} from '../codemirror-panel/code-mirror-viewer';
import { type ICodeBlock } from '../types';
import AnsiText from './AnsiText';
import { useCodeSandbox } from './useCodeSandbox';

export default function SandboxPanel({
  codeBlock,
  generated,
  isCodeblockLoading,
}: {
  codeBlock: ICodeBlock;
  generated: GeneratedFile[];
  isCodeblockLoading: boolean;
}) {
  const { output, isLoading, execute } = useCodeSandbox();
  const [code, setCode] = useState(codeBlock.code);
  const diagnostics = useAtomValue(CodeMirrorDiagnosticsAtom);
  useEffect(() => {
    setCode(codeBlock.code);
  }, [codeBlock.code]);

  return (
    <ResizablePanelGroup direction="horizontal" className="h-full">
      <ResizablePanel defaultSize={40} className="relative  h-full">
        <ScrollArea className="relative h-full flex-1 " type="always">
          <Button
            size="icon"
            onClick={() =>
              void execute(
                {
                  ...codeBlock,
                  code: code,
                },
                generated ?? [],
              )
            }
            variant={'outline'}
            disabled={isLoading || diagnostics.length > 0}
            className="absolute -top-0 right-0 z-10 text-green-500 hover:bg-green-100 disabled:bg-gray-100 disabled:text-gray-300 disabled:opacity-100 disabled:shadow-none"
          >
            <PlayCircle strokeWidth={1.5} className=" h-6 w-6" />
          </Button>

          <CodeMirrorViewer
            lang={codeBlock.language}
            generatedFiles={generated}
            fileContent={{
              code: code,
              language: codeBlock.language,
              id: 'sandbox-editor',
            }}
            onContentChange={(value) => setCode(value)}
            shouldScrollDown={false}
          />
        </ScrollArea>
      </ResizablePanel>
      <ResizableHandle withHandle className="border-none !bg-inherit" />
      <ResizablePanel
        defaultSize={60}
        className="h-[calc(100%-60px)] border-[1px] border-gray-200"
      >
        <ScrollArea className="h-full" type="always">
          <div className="p-0">
            <div className=" flex items-center justify-between" />
            <div className="h-full min-h-[150px] rounded-md p-2 font-mono">
              {diagnostics.length > 0 && (
                <div className="mb-3 rounded-md bg-red-100 p-2 text-center text-xs text-red-500">
                  Runner is disabled. There are {diagnostics.length} errors in
                  your BAML code. {diagnostics.map((d) => d.message).join(', ')}
                </div>
              )}
              {isLoading ? (
                <div className="flex items-center justify-center gap-2 text-center font-mono text-sm text-gray-500">
                  <div className="h-4 w-4 animate-spin rounded-full border-2 border-gray-300 border-t-gray-600" />
                  Running...
                </div>
              ) : output.length === 0 ? (
                <div className="text-center font-mono text-sm text-gray-500">
                  Terminal output. Waiting for execution...
                </div>
              ) : null}
              {output.map((item, i) => (
                <AnsiText
                  key={i}
                  text={item.line}
                  className="text-balance text-xs"
                />
              ))}
            </div>
          </div>
        </ScrollArea>
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
