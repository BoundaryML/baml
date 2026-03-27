import { useState, useCallback, type FC, type RefObject } from 'react';
import { Copy, Check, Loader2 } from 'lucide-react';
import { Button } from './ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './ui/tooltip';
import { cn } from '../lib/utils';

interface CopyButtonProps {
  text?: string;
  /** Copy innerText from this element instead of `text`. Takes precedence over `text`. */
  textRef?: RefObject<HTMLElement | null>;
  className?: string;
  iconSize?: number;
}

export const CopyButton: FC<CopyButtonProps> = ({ text, textRef, className = '', iconSize = 14 }) => {
  const [state, setState] = useState<'idle' | 'copying' | 'copied'>('idle');

  const handleCopy = useCallback(() => {
    const content = textRef?.current?.innerText ?? text ?? '';
    setState('copying');
    navigator.clipboard.writeText(content).then(() => {
      setState('copied');
      setTimeout(() => setState('idle'), 1500);
    }, () => {
      setState('idle');
    });
  }, [text, textRef]);

  const Icon = state === 'copied' ? Check : state === 'copying' ? Loader2 : Copy;

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className={cn('h-7 w-7 transition-opacity opacity-0 group-hover:opacity-100', className)}
            onClick={handleCopy}
          >
            <Icon size={iconSize} className={state === 'copying' ? 'animate-spin' : ''} />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{state === 'copied' ? 'Copied!' : 'Copy'}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};
