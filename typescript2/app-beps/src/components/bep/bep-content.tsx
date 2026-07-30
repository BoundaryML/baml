"use client";

import { MarkdownHooks } from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import { ShikiCodeBlock } from "@/components/ui/shiki-code-block";
import { MermaidDiagram } from "@/components/ui/mermaid-diagram";
import Link from "next/link";
import { ReactNode, isValidElement, useMemo } from "react";
import type { Components } from "react-markdown";
import type { Element } from "hast";
import { BepLinkContext, resolveBepLink } from "@/lib/bep-link-resolver";
import {
  extractHeadings,
  slugifyHeading,
  stripFrontmatter,
} from "@/lib/heading-utils";

interface BepContentProps {
  content: string;
  linkContext?: BepLinkContext;
}

function getTextContent(node: unknown): string {
  if (!node || typeof node !== "object") return "";
  if ("value" in node && typeof (node as { value: unknown }).value === "string") {
    return (node as { value: string }).value;
  }
  if (
    "children" in node &&
    Array.isArray((node as { children: unknown[] }).children)
  ) {
    return (node as { children: unknown[] }).children.map(getTextContent).join("");
  }
  return "";
}

function getHeadingText(children: ReactNode): string {
  if (children === null || children === undefined) return "";
  if (typeof children === "string" || typeof children === "number") {
    return String(children);
  }
  if (Array.isArray(children)) {
    return children.map((child) => getHeadingText(child)).join("");
  }
  if (isValidElement(children)) {
    return getHeadingText(
      (children.props as { children?: ReactNode }).children ?? ""
    );
  }
  return "";
}

/**
 * Deterministic heading-id lookup, precomputed from the raw markdown.
 *
 * Ids must not depend on render order or count: react-markdown re-invokes
 * the heading components on every re-render, so a mutable dedup counter in
 * a closure drifts (summary → summary-2 → …) and breaks TOC anchors. We
 * key by source line (exact) with heading text as a fallback.
 */
interface HeadingIdLookup {
  byLine: Map<number, string>;
  byText: Map<string, string>;
}

function buildHeadingIdLookup(content: string): HeadingIdLookup {
  const byLine = new Map<number, string>();
  const byText = new Map<string, string>();
  for (const heading of extractHeadings(content)) {
    byLine.set(heading.line, heading.id);
    // First occurrence wins for the text fallback
    if (!byText.has(heading.text)) {
      byText.set(heading.text, heading.id);
    }
  }
  return { byLine, byText };
}

function createHeadingComponent(
  tag: "h1" | "h2" | "h3" | "h4",
  className: string,
  lookup: HeadingIdLookup
): Components["h1"] {
  const Heading = ({
    children,
    node,
  }: {
    children?: ReactNode;
    node?: Element;
  }) => {
    const headingText = getHeadingText(children).trim();
    const line = node?.position?.start.line;
    const id =
      (line !== undefined ? lookup.byLine.get(line) : undefined) ??
      lookup.byText.get(headingText) ??
      (headingText ? slugifyHeading(headingText) || undefined : undefined);
    const Tag = tag;
    return (
      <Tag id={id} className={className}>
        {children}
      </Tag>
    );
  };
  return Heading;
}

function createComponents(
  lookup: HeadingIdLookup,
  linkContext?: BepLinkContext
): Components {
  return {
    h1: createHeadingComponent(
      "h1",
      "text-3xl font-bold mt-8 mb-4 first:mt-0 scroll-mt-24",
      lookup
    ),
    h2: createHeadingComponent(
      "h2",
      "text-2xl font-semibold mt-6 mb-3 pb-2 border-b border-border scroll-mt-24",
      lookup
    ),
    h3: createHeadingComponent(
      "h3",
      "text-xl font-semibold mt-5 mb-2 scroll-mt-24",
      lookup
    ),
    h4: createHeadingComponent(
      "h4",
      "text-lg font-medium mt-4 mb-2 scroll-mt-24",
      lookup
    ),
    pre: ({ children, node }) => {
      const codeElement = node?.children?.find(
        (child): child is typeof child & { tagName: string } =>
          "tagName" in child && child.tagName === "code"
      );

      if (codeElement && "properties" in codeElement) {
        const className =
          (codeElement.properties?.className as string[] | undefined)?.[0] || "";
        const language = className.replace(/^language-/, "");
        const code = getTextContent(codeElement).replace(/\n$/, "");
        if (language === "mermaid") {
          return <MermaidDiagram code={code} />;
        }
        return <ShikiCodeBlock code={code} language={language} showLineNumbers />;
      }

      return (
        <pre className="my-5 overflow-x-auto rounded-xl bg-code-bg p-5 font-mono text-sm text-code-fg">
          {children}
        </pre>
      );
    },
    code: ({ className, children }) => {
      const isInline = !className;
      if (isInline) {
        return (
          <code className="rounded bg-code-bg px-1.5 py-0.5 font-mono text-[0.875em] text-code-fg border border-code-border">
            {children}
          </code>
        );
      }
      return <code className={className}>{children}</code>;
    },
    a: ({ href, children }) => {
      const resolved = resolveBepLink(href, linkContext);
      const className =
        "text-primary underline underline-offset-4 hover:text-primary/80 transition-colors";

      if (resolved.isInternalBepLink) {
        return (
          <Link href={resolved.href} className={className}>
            {children}
          </Link>
        );
      }

      return (
        <a
          href={resolved.href || href}
          className={className}
          target={resolved.isExternal ? "_blank" : undefined}
          rel={resolved.isExternal ? "noopener noreferrer" : undefined}
        >
          {children}
        </a>
      );
    },
  };
}

export function BepContent({ content, linkContext }: BepContentProps) {
  const contentWithoutFrontmatter = useMemo(
    () => stripFrontmatter(content ?? ""),
    [content]
  );
  const components = useMemo(
    () =>
      createComponents(
        buildHeadingIdLookup(contentWithoutFrontmatter),
        linkContext
      ),
    [contentWithoutFrontmatter, linkContext]
  );

  if (!content) {
    return <div className="text-muted-foreground">No content</div>;
  }

  return (
    <article
      data-bep-content
      className="prose prose-sm sm:prose-base lg:prose-lg dark:prose-invert max-w-none prose-code:before:content-none prose-code:after:content-none"
    >
      <MarkdownHooks 
        remarkPlugins={[remarkGfm]} 
        rehypePlugins={[rehypeRaw]}
        components={components}
      >
        {contentWithoutFrontmatter}
      </MarkdownHooks>
    </article>
  );
}
