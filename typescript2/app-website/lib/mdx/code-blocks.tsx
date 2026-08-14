'use client';

import { Copy } from 'lucide-react';
import React, { useState } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface CodeBlocksProps {
  children: React.ReactNode;
}

interface CodeElementProps {
  children?: React.ReactNode;
  className?: string;
  filename?: string;
  meta?: string;
  language?: string;
}

interface CodeElement extends React.ReactElement<CodeElementProps> {
  data?: {
    meta?: string;
  };
}

interface PreElementProps {
  children: React.ReactElement;
  filename?: string;
  className?: string;
}

// Helper to extract metadata from code blocks
function getCodeBlockMetadata(child: React.ReactNode) {
  if (!React.isValidElement(child)) return null;

  const element = child as React.ReactElement<PreElementProps>;
  if (element.type === 'pre') {
    return {
      filename: element.props.filename,
      language: element.props.className?.replace('language-', '') || 'text',
    };
  }

  return null;
}

export function CodeBlocks({ children }: CodeBlocksProps) {
  const [copied, setCopied] = useState(false);
  const [activeTab, setActiveTab] = useState<number>(0);

  const childArray = React.Children.toArray(children);
  const metadata = childArray
    .map((child) => getCodeBlockMetadata(child))
    .filter(Boolean);

  const copyToClipboard = async () => {
    const activePreElement = document.querySelector(
      `[data-tab="${activeTab}"] pre`,
    );
    if (!activePreElement) return;

    await navigator.clipboard.writeText(activePreElement.textContent || '');
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // If there's only one child, render it directly without tabs
  if (childArray.length <= 1) {
    return (
      <div className="relative my-6 rounded-lg border bg-background">
        <div className="flex items-center justify-between border-b px-4">
          <div className="h-12 flex items-center">
            <span className="text-sm text-muted-foreground">
              {metadata[0]?.filename || 'Code'}
            </span>
          </div>
          <button
            className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
            onClick={copyToClipboard}
            type="button"
          >
            <Copy className="h-4 w-4" />
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>
        <div data-tab="0">{children}</div>
      </div>
    );
  }

  return (
    <div className="relative my-6 rounded-lg border bg-background">
      <Tabs
        className="relative"
        onValueChange={(v) => setActiveTab(Number.parseInt(v))}
        value={activeTab.toString()}
      >
        <div className="flex items-center justify-between border-b px-4">
          <TabsList className="h-12">
            {childArray.map((_, index) => (
              <TabsTrigger
                className="relative h-8 rounded-none border-b-2 border-b-transparent px-4 pb-3 pt-2 font-medium text-muted-foreground hover:text-foreground data-[state=active]:border-b-primary data-[state=active]:text-foreground"
                key={index}
                value={index.toString()}
              >
                {metadata[index]?.filename || `File ${index + 1}`}
              </TabsTrigger>
            ))}
          </TabsList>
          <button
            className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
            onClick={copyToClipboard}
          >
            <Copy className="h-4 w-4" />
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>

        {childArray.map((child, index) => (
          <TabsContent
            className="border-none p-0"
            key={index}
            value={index.toString()}
          >
            <div data-tab={index}>{child}</div>
          </TabsContent>
        ))}
      </Tabs>
    </div>
  );
}
