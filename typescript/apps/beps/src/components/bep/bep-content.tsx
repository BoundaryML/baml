"use client";

import { MarkdownHooks } from "react-markdown";
import remarkGfm from "remark-gfm";
import { ShikiCodeBlock } from "@/components/ui/shiki-code-block";
import { Children, isValidElement } from "react";
import type { Components } from "react-markdown";

interface BepContentProps {
  content: string;
}

// Define components inline to avoid import issues
const components: Components = {
  pre: ({ children, node }) => {
    // react-markdown passes the hast node - check if it contains a code element
    const codeElement = node?.children?.find(
      (child): child is typeof child & { tagName: string } => 
        'tagName' in child && child.tagName === 'code'
    );
    
    if (codeElement && 'properties' in codeElement) {
      const className = (codeElement.properties?.className as string[] | undefined)?.[0] || '';
      const language = className.replace(/^language-/, '');
      
      // Get text content from the code element
      const getTextContent = (node: unknown): string => {
        if (!node || typeof node !== 'object') return '';
        if ('value' in node && typeof (node as {value: unknown}).value === 'string') {
          return (node as {value: string}).value;
        }
        if ('children' in node && Array.isArray((node as {children: unknown[]}).children)) {
          return (node as {children: unknown[]}).children.map(getTextContent).join('');
        }
        return '';
      };
      
      const code = getTextContent(codeElement).replace(/\n$/, '');
      return <ShikiCodeBlock code={code} language={language} showLineNumbers />;
    }

    return <pre className="my-5 overflow-x-auto rounded-xl bg-muted p-5 font-mono text-sm">{children}</pre>;
  },
  code: ({ className, children }) => {
    const isInline = !className;
    if (isInline) {
      return (
        <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.875em] text-foreground border border-border">
          {children}
        </code>
      );
    }
    return <code className={className}>{children}</code>;
  },
};

export function BepContent({ content }: BepContentProps) {
  if (!content) {
    return <div className="text-muted-foreground">No content</div>;
  }

  // Strip frontmatter (content between --- markers at the start)
  const contentWithoutFrontmatter = content.replace(/^---[\s\S]*?---\n*/, '');

  return (
    <article 
      data-bep-content
      className="prose prose-sm sm:prose-base lg:prose-lg dark:prose-invert max-w-none prose-code:before:content-none prose-code:after:content-none"
    >
      <MarkdownHooks remarkPlugins={[remarkGfm]} components={components}>
        {contentWithoutFrontmatter}
      </MarkdownHooks>
    </article>
  );
}
