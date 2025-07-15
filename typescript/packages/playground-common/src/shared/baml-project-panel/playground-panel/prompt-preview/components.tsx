import { Button } from '@baml/ui/button';
import { cn } from '@baml/ui/lib/utils';
import { Check, Copy, Loader2 } from 'lucide-react';
import { useState } from 'react';

export const Loader: React.FC<{ message?: string; className?: string }> = ({
  message,
  className,
}) => {
  return (
    <div
      className={cn(
        'flex gap-2 justify-center items-center text-gray-500',
        className,
      )}
    >
      <Loader2 className="animate-spin" />
      {message}
    </div>
  );
};

export const ErrorMessage: React.FC<{ error: string }> = ({ error }) => {
  return (
    <pre
      className="px-2 py-1 w-full font-mono text-red-500 whitespace-pre-wrap rounded-lg"
      style={{
        wordBreak: 'normal',
        overflowWrap: 'anywhere',
      }}
    >
      {error}
    </pre>
  );
};

export const WithCopyButton: React.FC<{
  children: React.ReactNode;
  text: string;
}> = ({ children, text }) => {
  const [copyState, setCopyState] = useState<'copying' | 'copied' | 'idle'>(
    'idle',
  );

  return (
    <div className="relative group">
      {/* Solid overlay to block text interference behind button */}
      <div className="absolute top-1 right-1 w-16 h-8 z-20 pointer-events-none select-none bg-accent" />
      
      {copyState === 'idle' && (
        <Button
          onClick={() => {
            setCopyState('copying');
            void navigator.clipboard.writeText(text).then(() => {
              setCopyState('copied');
              setTimeout(() => {
                setCopyState('idle');
              }, 1000);
            });
          }}
          className="absolute top-1 right-1 opacity-0 transition-opacity group-hover:opacity-100 z-30 select-none pointer-events-auto"
          variant="outline"
          size="sm"
          title="Copy to clipboard"
        >
          <Copy className="w-4 h-4" />
        </Button>
      )}
      {copyState === 'copying' && (
        <div className="flex absolute top-1 right-1 justify-center items-center z-30 select-none pointer-events-auto">
          <Button variant="outline" size="sm" disabled>
            <Loader2 className="w-4 h-4 animate-spin" />
          </Button>
        </div>
      )}
      {copyState === 'copied' && (
        <div className="flex absolute top-1 right-1 z-30 select-none pointer-events-auto">
          <div className="flex items-center gap-1 px-2 py-1 text-sm font-medium text-green-700 bg-green-100 border border-green-300 rounded-md dark:text-green-400 dark:bg-green-950/95 dark:border-green-800 shadow-sm">
            <Check className="w-4 h-4" />
            Copied!
          </div>
        </div>
      )}
      {children}
    </div>
  );
};
