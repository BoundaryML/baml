import { Button } from '@/components/ui/button';
import { Check, Copy, Loader2 } from 'lucide-react';
import { useState } from 'react';

export const Loader: React.FC<{ message?: string }> = ({ message }) => {
  return (
    <div className="flex items-center justify-center gap-2 text-gray-500">
      <Loader2 className="animate-spin" />
      {message}
    </div>
  );
};

export const ErrorMessage: React.FC<{ error: string }> = ({ error }) => {
  return (
    <pre
      className="w-full whitespace-pre-wrap rounded-lg px-2 py-1 font-mono text-red-500"
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
    <div className="group relative">
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
          className="absolute right-1 top-1 h-8 w-8 p-0 opacity-0 transition-opacity group-hover:opacity-100"
          variant="ghost"
          size="icon"
          title="Copy to clipboard"
        >
          <Copy className="h-4 w-4" />
        </Button>
      )}
      {copyState === 'copying' && (
        <div className="absolute right-1 top-1 flex h-8 w-8 items-center justify-center p-0">
          <Loader />
        </div>
      )}
      {copyState === 'copied' && (
        <div className="absolute right-1 top-1 z-10 flex h-8 flex-row items-center justify-center gap-1 rounded-md bg-muted px-2 text-green-500">
          <Check className="h-4 w-4" /> Copied!
        </div>
      )}
      {children}
    </div>
  );
};
