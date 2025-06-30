import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@baml/ui/collapsible';
import { cn } from '@baml/ui/lib/utils';
import type {
  WasmChatMessage,
  WasmTestCase,
} from '@gloo-ai/baml-schema-wasm-web';
import { ChevronDown, ChevronsUpDown, ChevronUp } from 'lucide-react';
import { useState } from 'react';
import { getFirstLine } from './highlight-utils';
import { PromptStats } from './prompt-stats';
import { RenderPart } from './render-part';

interface CollapsibleMessageProps {
  part: WasmChatMessage;
  partIndex: number;
  testCase?: WasmTestCase;
}

export const CollapsibleMessage: React.FC<CollapsibleMessageProps> = ({
  part,
  partIndex,
  testCase,
}) => {
  const [open, setOpen] = useState(false);
  const firstLine = getFirstLine(part.parts);
  const statsText = part.parts
    .map((part: any) => part.as_text() ?? '')
    .join('\n');

  return (
    <div
      className={cn('border-l-4 pl-2 rounded', {
        'border-[var(--vscode-charts-blue)]': part.role === 'assistant',
        'border-[var(--vscode-charts-green)]': part.role === 'user',
        'border-[var(--vscode-charts-gray)]': part.role === 'system',
        'border-[var(--vscode-charts-yellow)]':
          part.role !== 'assistant' &&
          part.role !== 'user' &&
          part.role !== 'system',
      })}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        <CollapsibleTrigger
          className={cn(
            'flex w-full items-center justify-between p-3 transition-colors',
            'data-[state=closed]:bg-card rounded-t data-[state=closed]:hover:bg-card/80 cursor-pointer data-[state=open]:hover:bg-card/80',
          )}
        >
          <div className="flex flex-col items-start gap-1 flex-1 overflow-hidden min-w-0">
            <div className="flex items-center w-full justify-between">
              <div className="text-xs text-muted-foreground">
                {part.role.charAt(0).toUpperCase() + part.role.slice(1)}
              </div>
              {open ? (
                <ChevronUp className="size-4 ml-4 flex-shrink-0" />
              ) : (
                <ChevronDown className="size-4 ml-4 flex-shrink-0" />
              )}
            </div>
            {!open && firstLine && (
              <div className="text-xs truncate whitespace-nowrap w-full text-left font-mono mt-2">
                {firstLine}
              </div>
            )}
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent className="space-y-3">
          {part.parts.map((part, index) => (
            <div
              key={`${partIndex}-${
                // biome-ignore lint/suspicious/noArrayIndexKey: <explanation>
                index
              }`}
            >
              <RenderPart part={part} testCase={testCase} />
            </div>
          ))}
        </CollapsibleContent>
      </Collapsible>
      <PromptStats text={statsText} />
    </div>
  );
};
