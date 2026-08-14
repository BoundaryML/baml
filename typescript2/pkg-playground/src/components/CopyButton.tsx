// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing exported component path.
import { Check, Copy, Loader2 } from 'lucide-react';
import { type FC, type RefObject, useCallback, useState } from 'react';
import { cn } from '../lib/utils';
import { Button } from './ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './ui/tooltip';

interface CopyButtonProps {
  text?: string;
  /** Lazily compute copy text when the button is clicked. */
  getText?: () => string;
  /** Copy innerText from this element instead of `text`. Takes precedence over `text`. */
  textRef?: RefObject<HTMLElement | null>;
  className?: string;
  iconSize?: number;
}

export const CopyButton: FC<CopyButtonProps> = ({
  text,
  getText,
  textRef,
  className = '',
  iconSize = 14,
}) => {
  const [state, setState] = useState<'idle' | 'copying' | 'copied'>('idle');

  const handleCopy = useCallback(() => {
    const content = textRef?.current?.innerText ?? getText?.() ?? text ?? '';
    setState('copying');
    navigator.clipboard.writeText(content).then(
      () => {
        setState('copied');
        setTimeout(() => setState('idle'), 1500);
      },
      () => {
        setState('idle');
      },
    );
  }, [getText, text, textRef]);

  const Icon =
    state === 'copied' ? Check : state === 'copying' ? Loader2 : Copy;

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            className={cn(
              'h-7 w-7 transition-opacity opacity-0 group-hover:opacity-100',
              className,
            )}
            onClick={handleCopy}
            size="icon"
            variant="ghost"
          >
            <Icon
              className={state === 'copying' ? 'animate-spin' : ''}
              size={iconSize}
            />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {state === 'copied' ? 'Copied!' : 'Copy'}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};
