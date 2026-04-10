'use client';

import { FileIcon } from 'lucide-react';
import type React from 'react';
import { useState } from 'react';
import { cn } from '@/lib/utils';

export type CodeSnippet = {
  filename: string;
  code?: React.ReactNode;
  language?: string;
};

export type CodeBlockProps = {
  children?: React.ReactNode;
  className?: string;
  filename?: string;
  snippets?: CodeSnippet[];
  onTabChange?: (index: number) => void;
} & React.HTMLProps<HTMLDivElement>;

function CodeBlock({
  children,
  className,
  filename,
  snippets,
  onTabChange,
  ...props
}: CodeBlockProps) {
  const [activeTab, setActiveTab] = useState(0);

  // Support backward compatibility - if filename is provided, use single file mode
  const isMultiFile = snippets && snippets.length > 0;
  const hasHeader = filename || isMultiFile;

  return (
    <div
      className={cn(
        'not-prose flex w-full flex-col overflow-clip border',
        'border-border bg-card text-card-foreground rounded-xl',
        className,
      )}
      {...props}
    >
      {hasHeader && (
        <div className="flex items-center border-b border-border bg-accent text-sm text-foreground">
          {isMultiFile ? (
            // Multiple files - show tabs
            <div className="flex items-center w-full">
              {snippets.map((snippet, index) => (
                <button
                  className={cn(
                    'flex items-center px-3 py-2 hover:bg-accent-foreground/5 transition-colors border-r border-border',
                    activeTab === index && 'bg-background',
                  )}
                  key={snippet.filename}
                  onClick={() => {
                    setActiveTab(index);
                    onTabChange?.(index);
                  }}
                  type="button"
                >
                  <FileIcon className="mr-2 size-4" />
                  {snippet.filename}
                </button>
              ))}
            </div>
          ) : (
            // Single file - show filename only
            <div className="flex items-center p-2">
              <FileIcon className="mr-2 size-4" />
              {filename}
            </div>
          )}
        </div>
      )}
      {isMultiFile ? (
        // Render the active snippet's code
        <div className="w-full">{snippets[activeTab].code}</div>
      ) : (
        // Render children for backward compatibility
        children
      )}
    </div>
  );
}

export type CodeBlockCodeProps = {
  code: string;
  language?: string;
  theme?: string;
  className?: string;
} & React.HTMLProps<HTMLDivElement>;

function CodeBlockCode({
  code,
  language = 'tsx',
  theme = 'github-light',
  className,
  ...props
}: CodeBlockCodeProps) {
  const classNames = cn(
    'w-full overflow-x-auto text-[13px] [&>pre]:px-4 [&>pre]:py-4',
    className,
  );

  return (
    <div className={classNames} {...props}>
      <pre>
        <code>{code}</code>
      </pre>
    </div>
  );
}

export type CodeBlockGroupProps = React.HTMLAttributes<HTMLDivElement>;

function CodeBlockGroup({
  children,
  className,
  ...props
}: CodeBlockGroupProps) {
  return (
    <div
      className={cn('flex items-center justify-between', className)}
      {...props}
    >
      {children}
    </div>
  );
}

export { CodeBlockGroup, CodeBlockCode, CodeBlock };
