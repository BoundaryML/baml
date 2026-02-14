import type { Components } from "react-markdown";
import { ShikiCodeBlock } from "@/components/ui/shiki-code-block";
import { isValidElement, Children } from "react";

export const markdownComponents: Components = {
  h1: ({ children }) => (
    <h1 className="text-3xl font-bold mt-8 mb-4 first:mt-0">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="text-2xl font-semibold mt-6 mb-3 pb-2 border-b border-border">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="text-xl font-semibold mt-5 mb-2">{children}</h3>
  ),
  h4: ({ children }) => (
    <h4 className="text-lg font-medium mt-4 mb-2">{children}</h4>
  ),
  p: ({ children }) => <p className="my-3 leading-7">{children}</p>,
  ul: ({ children }) => <ul className="my-3 ml-6 list-disc space-y-1">{children}</ul>,
  ol: ({ children }) => <ol className="my-3 ml-6 list-decimal space-y-1">{children}</ol>,
  li: ({ children }) => <li className="leading-7">{children}</li>,
  blockquote: ({ children }) => (
    <blockquote className="my-4 border-l-4 border-amber-500/60 pl-4 italic text-muted-foreground bg-amber-500/5 py-2 rounded-r-md">
      {children}
    </blockquote>
  ),
  code: ({ className, children }) => {
    // Inline code (no className)
    const isInline = !className;
    if (isInline) {
      return (
        <code className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-[0.875em] text-foreground border border-border">
          {children}
        </code>
      );
    }
    // Block code - will be rendered by ShikiCodeBlock in pre component
    return (
      <code className={className}>
        {children}
      </code>
    );
  },
  pre: ({ children }) => {
    // Extract code content and language from the code child
    const childArray = Children.toArray(children);
    const codeChild = childArray.find(
      (child) => isValidElement(child) && child.type === "code"
    );

    if (isValidElement(codeChild)) {
      const className = (codeChild.props as { className?: string }).className || "";
      const language = className.replace(/^language-/, "");
      const code = String((codeChild.props as { children?: unknown }).children || "").replace(/\n$/, "");

      return <ShikiCodeBlock code={code} language={language} showLineNumbers />;
    }

    // Fallback for non-code pre content
    return (
      <pre className="my-5 overflow-x-auto rounded-xl bg-muted p-5 font-mono text-sm text-foreground border border-border shadow-sm">
        {children}
      </pre>
    );
  },
  a: ({ href, children }) => (
    <a
      href={href}
      className="text-primary underline underline-offset-4 hover:text-primary/80 transition-colors"
      target={href?.startsWith("http") ? "_blank" : undefined}
      rel={href?.startsWith("http") ? "noopener noreferrer" : undefined}
    >
      {children}
    </a>
  ),
  table: ({ children }) => (
    <div className="my-5 overflow-x-auto rounded-lg border border-border shadow-sm">
      <table className="w-full border-collapse">
        {children}
      </table>
    </div>
  ),
  thead: ({ children }) => <thead className="bg-muted/50">{children}</thead>,
  th: ({ children }) => (
    <th className="border-b border-border px-4 py-3 text-left font-semibold text-sm uppercase tracking-wider text-muted-foreground">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border-b border-border/50 px-4 py-3 text-sm">{children}</td>
  ),
  hr: () => <hr className="my-8 border-border" />,
  img: ({ src, alt }) => (
    <img src={src} alt={alt} className="my-4 max-w-full rounded-xl shadow-md" />
  ),
  strong: ({ children }) => (
    <strong className="font-semibold text-foreground">{children}</strong>
  ),
  em: ({ children }) => <em className="italic text-muted-foreground">{children}</em>,
};
