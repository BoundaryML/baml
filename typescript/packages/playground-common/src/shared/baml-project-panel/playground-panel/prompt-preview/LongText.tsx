import { ScrollArea } from '@baml/ui/scroll-area';
import { ChevronDown } from 'lucide-react';
import { ChevronUp } from 'lucide-react';
import { useState } from 'react';
import { useMemo } from 'react';

const RenderText: React.FC<{
  text: string;
  asJson?: boolean;
  debugMode?: boolean;
}> = ({ text, asJson, debugMode }) => {
  const isDebugMode = debugMode;
  const isLongText = useMemo(() => text.split('\n').length > 18, [text]);
  const [isFullTextVisible, setIsFullTextVisible] = useState(false);

  return (
    <div className="flex flex-col">
      {isDebugMode && (
        <div className="flex flex-row gap-4 justify-start items-center px-2 py-2 text-xs border-b border-border bg-muted text-muted-foreground">
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
      <ScrollArea className="relative flex-1 p-2 bg-muted/50" type="always">
        <pre
          className={`whitespace-pre-wrap text-xs  ${isFullTextVisible ? 'max-h-96' : 'max-h-72'}`}
        >
          {asJson ? JSON.stringify(text, null, 2) : text}
        </pre>

        {isLongText && (
          <button
            onClick={() => setIsFullTextVisible(!isFullTextVisible)}
            className="flex absolute right-0 bottom-0 gap-1 items-center p-2 text-xs rounded-tr-md rounded-bl-md transition-colors bg-muted/50 text-muted-foreground hover:text-foreground"
          >
            {isFullTextVisible ? (
              <>
                Show less
                <ChevronUp className="w-3 h-3" />
              </>
            ) : (
              <>
                Show more
                <ChevronDown className="w-3 h-3" />
              </>
            )}
          </button>
        )}
      </ScrollArea>
    </div>
  );
};

export default RenderText;
