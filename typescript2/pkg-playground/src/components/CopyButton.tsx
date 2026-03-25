import { useState, useCallback, type FC } from 'react';
import { Copy, Check, Loader2 } from 'lucide-react';

interface CopyButtonProps {
  text: string;
  className?: string;
  iconSize?: number;
}

export const CopyButton: FC<CopyButtonProps> = ({ text, className = '', iconSize = 14 }) => {
  const [state, setState] = useState<'idle' | 'copying' | 'copied'>('idle');

  const handleCopy = useCallback(() => {
    setState('copying');
    navigator.clipboard.writeText(text).then(() => {
      setState('copied');
      setTimeout(() => setState('idle'), 1500);
    }, () => {
      setState('idle');
    });
  }, [text]);

  const Icon = state === 'copied' ? Check : state === 'copying' ? Loader2 : Copy;

  return (
    <button
      onClick={handleCopy}
      className={`p-1 rounded hover:bg-vsc-hover text-vsc-text-muted hover:text-vsc-text transition-opacity opacity-0 group-hover:opacity-100 ${className}`}
      title={state === 'copied' ? 'Copied!' : 'Copy'}
    >
      <Icon size={iconSize} className={state === 'copying' ? 'animate-spin' : ''} />
    </button>
  );
};
