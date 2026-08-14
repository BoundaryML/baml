import { ScrollArea } from '@/components/ui/scroll-area';
import { atom, useAtomValue } from 'jotai';
import { ChevronDown, ChevronUp } from 'lucide-react';
import { useMemo, useState } from 'react';
import { renderModeAtom } from '../preview-toolbar';

export const isDebugModeAtom = atom((get) => get(renderModeAtom) === 'tokens');

export const RenderText: React.FC<{
  text: string;
  highlightChunks?: string[];
}> = ({ text, highlightChunks = [] }) => {
  const isDebugMode = useAtomValue(isDebugModeAtom);
  const isLongText = useMemo(() => text.split('\n').length > 5, [text]);
  const [isFullTextVisible, setIsFullTextVisible] = useState(false);

  const highlightedText = useMemo(() => {
    if (!highlightChunks?.length) return text;

    let result = text;
    highlightChunks.forEach((chunk) => {
      if (!chunk) return;
      const regex = new RegExp(
        `(${chunk.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`,
        'g',
      );
      result = result.replace(
        regex,
        '<mark class="bg-yellow-200/50 rounded px-0.5">$1</mark>',
      );
    });
    return result;
  }, [text, highlightChunks]);

  return (
    <div className="flex flex-col">
      {isDebugMode && (
        <div className="flex flex-row items-center justify-start gap-4 border-b border-border bg-muted px-3 py-2 text-xs text-muted-foreground">
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground/60">Characters:</span>
            <span className="font-medium">{text.length}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground/60">Words:</span>
            <span className="font-medium">
              {text.split(/\s+/).filter(Boolean).length}
            </span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground/60">Lines:</span>
            <span className="font-medium">{text.split('\n').length}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground/60">Tokens (est.):</span>
            <span className="font-medium">{Math.ceil(text.length / 4)}</span>
          </div>
        </div>
      )}
      <ScrollArea
        className="relative flex-1 bg-muted/50 p-2 pb-6"
        type="always"
      >
        <pre
          className={`whitespace-pre-wrap text-xs  ${isFullTextVisible ? 'max-h-96' : 'max-h-48'}`}
          dangerouslySetInnerHTML={{ __html: highlightedText }}
        />

        {isLongText && (
          <button
            onClick={() => setIsFullTextVisible(!isFullTextVisible)}
            className="absolute bottom-0 right-0 flex items-center gap-1 rounded-bl-md rounded-tr-md bg-muted/50 p-2 text-xs text-muted-foreground transition-colors hover:text-foreground"
          >
            {isFullTextVisible ? (
              <>
                Show less
                <ChevronUp className="h-3 w-3" />
              </>
            ) : (
              <>
                Show more
                <ChevronDown className="h-3 w-3" />
              </>
            )}
          </button>
        )}
      </ScrollArea>
    </div>
  );
};
