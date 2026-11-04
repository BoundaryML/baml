/** biome-ignore-all lint/suspicious/noArrayIndexKey: the tabs are a post's fixed code blocks, in source order */
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

// A dark code panel, in both themes: the
// filename row on top, the highlighted code under it with no frame of its own.
const PANEL =
  'not-prose relative my-7 overflow-hidden rounded-xl border border-white/10 bg-[#0F0F0F] dark:bg-[#17191b] [&_pre]:!m-0 [&_pre]:!rounded-none [&_pre]:!border-0 [&_pre]:!bg-transparent [&_pre]:overflow-x-auto [&_pre]:px-5 [&_pre]:py-5 [&_pre]:font-mono [&_pre]:text-[13px] [&_pre]:leading-[22px]';
const CHROME =
  'flex items-center justify-between gap-5 border-b border-[#2A2E28] px-5 py-3 font-mono text-xs';
const COPY =
  'flex shrink-0 items-center gap-1.5 font-mono text-xs text-[#6E756A] transition-colors hover:text-white';

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
      <div className={PANEL}>
        <div className={CHROME}>
          <span className="min-w-0 truncate text-white">
            {metadata[0]?.filename || 'Code'}
          </span>
          <button className={COPY} onClick={copyToClipboard} type="button">
            <Copy className="h-3.5 w-3.5" />
            {copied ? 'copied' : 'copy'}
          </button>
        </div>
        <div data-tab="0">{children}</div>
      </div>
    );
  }

  return (
    <div className={PANEL}>
      <Tabs
        className="relative gap-0"
        onValueChange={(v) => setActiveTab(Number.parseInt(v))}
        value={activeTab.toString()}
      >
        <div className={CHROME}>
          <TabsList className="h-auto gap-1 bg-transparent p-0">
            {childArray.map((_, index) => (
              <TabsTrigger
                className="h-auto rounded-md border-0 px-2.5 py-1.5 font-mono text-xs font-normal text-[#6E756A] shadow-none hover:text-white data-[state=active]:bg-white/7 data-[state=active]:text-white data-[state=active]:shadow-none"
                key={index}
                value={index.toString()}
              >
                {metadata[index]?.filename || `File ${index + 1}`}
              </TabsTrigger>
            ))}
          </TabsList>
          <button className={COPY} onClick={copyToClipboard} type="button">
            <Copy className="h-3.5 w-3.5" />
            {copied ? 'copied' : 'copy'}
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
