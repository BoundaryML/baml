import { ChevronDown, ChevronUp } from 'lucide-react';
import { useState, useMemo } from 'react';
import { cn } from '@baml/ui/lib/utils';
import { Button } from '@baml/ui/button';

interface TruncatedStringProps {
  text: string;
  maxLength?: number;
  headLength?: number;
  tailLength?: number;
  className?: string;
  showStats?: boolean;
}

export const TruncatedString: React.FC<TruncatedStringProps> = ({
  text,
  maxLength = 1000,
  headLength = 400,
  tailLength = 400,
  className,
  showStats = false,
}) => {
  const [isExpanded, setIsExpanded] = useState(false);

  const { isTruncated, displayText } = useMemo(() => {
    if (text.length <= maxLength) {
      return { isTruncated: false, displayText: text };
    }

    if (isExpanded) {
      return { isTruncated: true, displayText: text };
    }

    const head = text.slice(0, headLength);
    const tail = text.slice(-tailLength);
    const truncatedText = `${head}\n\n... (${text.length - headLength - tailLength} characters hidden) ...\n\n${tail}`;
    
    return { isTruncated: true, displayText: truncatedText };
  }, [text, maxLength, headLength, tailLength, isExpanded]);

  return (
    <div className="relative group">
      {showStats && (
        <div className="flex flex-row gap-4 justify-start items-center px-2 py-2 text-xs border-b border-border bg-muted text-muted-foreground">
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground/60">Characters:</span>
            <span className="font-medium">{text.length}</span>
          </div>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground/60">Lines:</span>
            <span className="font-medium">{text.split('\n').length}</span>
          </div>
        </div>
      )}
      
      <div className="relative w-full">
        <div className={cn(
          'relative overflow-auto w-full',
          isExpanded ? 'max-h-[80vh]' : 'max-h-fit'
        )}>
          <pre
            className={cn(
              'whitespace-pre-wrap break-all text-xs w-full',
              className
            )}
          >
            {displayText}
          </pre>
        </div>

        {isTruncated && (
          <Button
            onClick={() => setIsExpanded(!isExpanded)}
            className="absolute bottom-1 right-1 opacity-0 transition-opacity group-hover:opacity-100 flex items-center gap-1 px-2 py-1 h-auto text-xs bg-muted text-muted-foreground hover:bg-muted/80 hover:text-foreground border border-border"
            size="sm"
          >
            {isExpanded ? (
              <>
                Show less
                <ChevronUp className="w-3 h-3" />
              </>
            ) : (
              <>
                Show all
                <ChevronDown className="w-3 h-3" />
              </>
            )}
          </Button>
        )}
      </div>
    </div>
  );
}; 