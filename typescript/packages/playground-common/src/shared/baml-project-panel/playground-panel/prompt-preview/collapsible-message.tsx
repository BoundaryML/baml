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
import { CopyButton } from '@baml/ui/custom/copy-button';

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
  const [open, setOpen] = useState(true);
  const firstLine = getFirstLine(part.parts);
  const statsText = part.parts
    .map((part: any) => part.as_text() ?? '')
    .join('\n');

  // Check for media types when no first line
  const getMediaIndicator = () => {
    if (firstLine) return firstLine;

    let hasImage = false;
    let hasAudio = false;
    let hasPdf = false;
    let hasVideo = false;

    for (const p of part.parts) {
      if (p.is_image?.()) hasImage = true;
      if (p.is_audio?.()) hasAudio = true;
      if (p.is_pdf?.()) hasPdf = true;
      if (p.is_video?.()) hasVideo = true;
    }

    const indicators: string[] = [];
    if (hasImage) indicators.push('[image]');
    if (hasAudio) indicators.push('[audio]');
    if (hasPdf) indicators.push('[pdf]');
    if (hasVideo) indicators.push('[video]');

    return indicators.length > 0 ? indicators.join(' ') : '';
  };

  const displayText = getMediaIndicator();

  return (
    <div
      className={cn('relative border-l-4 pl-2 rounded', {
        'border-chart-1': part.role === 'assistant',
        'border-chart-2': part.role === 'user',
        'border-chart-3': part.role === 'system',
        'border-chart-4':
          part.role !== 'assistant' &&
          part.role !== 'user' &&
          part.role !== 'system',
      })}
    >
      <Collapsible open={open} onOpenChange={setOpen}>
        <CollapsibleTrigger
          className={
            'flex w-full items-center justify-between p-3 transition-colors rounded-t hover:bg-accent/30 cursor-pointer bg-accent'
          }
        >
          <div className="flex flex-col items-start gap-1 flex-1 overflow-hidden min-w-0 w-full">
            <div className="flex items-center w-full justify-between gap-2">
              {/* Role on the left */}
              <div className="text-xs text-muted-foreground font-mono min-w-0 truncate">
                {part.role.charAt(0).toUpperCase() + part.role.slice(1)}
              </div>
              <div className="flex items-center gap-3 flex-shrink-0">
                {/* Copy button */}
                <CopyButton
                  text={part.parts.map((p: any) => p.as_text?.() ?? '').join('\n')}
                  size="sm"
                  variant="outline"
                  aria-label="Copy message"

                />
                {/* Expand/collapse icon */}
                {open ? (
                  <ChevronUp className="size-4 flex-shrink-0" />
                ) : (
                  <ChevronDown className="size-4 flex-shrink-0" />
                )}
              </div>
            </div>
            {/* Show first line or media indicator when collapsed */}
            {!open && displayText && (
              <div className="text-xs truncate whitespace-nowrap w-full text-left font-mono mt-2">
                {displayText}
              </div>
            )}
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
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
      <PromptStats text={statsText} parts={part.parts} />
    </div>
  );
};
