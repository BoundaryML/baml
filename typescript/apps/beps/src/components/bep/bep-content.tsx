"use client";

import { MarkdownHooks } from "react-markdown";
import remarkGfm from "remark-gfm";
import { ShikiCodeBlock } from "@/components/ui/shiki-code-block";
import Link from "next/link";
import { ReactNode, isValidElement, useMemo } from "react";
import type { Components } from "react-markdown";
import { BepLinkContext, resolveBepLink } from "@/lib/bep-link-resolver";

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

function slugifyHeading(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, "")
    .replace(/[\s_]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

function createHeadingComponent(
  tag: "h1" | "h2" | "h3" | "h4",
  className: string,
  getId: (headingText: string) => string | undefined
): Components["h1"] {
  const Heading = ({ children }: { children?: ReactNode }) => {
    const headingText = getHeadingText(children);
    const id = headingText ? getId(headingText) : undefined;
    const Tag = tag;
    return (
      <Tag id={id} className={className}>
        {children}
      </Tag>
    );
  };
  return Heading;
}

function createComponents(linkContext?: BepLinkContext): Components {
  const slugCounts = new Map<string, number>();
  const getUniqueId = (headingText: string) => {
    const base = slugifyHeading(headingText);
    if (!base) return undefined;
    const count = (slugCounts.get(base) ?? 0) + 1;
    slugCounts.set(base, count);
    return count === 1 ? base : `${base}-${count}`;
  };

  return {
    h1: createHeadingComponent(
      "h1",
      "text-3xl font-bold mt-8 mb-4 first:mt-0 scroll-mt-24",
      getUniqueId
    ),
    h2: createHeadingComponent(
      "h2",
      "text-2xl font-semibold mt-6 mb-3 pb-2 border-b border-border scroll-mt-24",
      getUniqueId
    ),
    h3: createHeadingComponent(
      "h3",
      "text-xl font-semibold mt-5 mb-2 scroll-mt-24",
      getUniqueId
    ),
    h4: createHeadingComponent(
      "h4",
      "text-lg font-medium mt-4 mb-2 scroll-mt-24",
      getUniqueId
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
  if (!content) {
    return <div className="text-muted-foreground">No content</div>;
  }

  const components = useMemo(() => createComponents(linkContext), [linkContext]);
  const contentWithoutFrontmatter = content.replace(/^---[\s\S]*?---\n*/, "");

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
