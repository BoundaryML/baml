import type { Components } from "react-markdown";

export const markdownComponents: Components = {
  h1: ({ children }) => (
    <h1 className="text-3xl font-bold mt-8 mb-4 first:mt-0">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="text-2xl font-semibold mt-6 mb-3 pb-2 border-b">{children}</h2>
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
    <blockquote className="my-4 border-l-4 border-muted-foreground/30 pl-4 italic text-muted-foreground">
      {children}
    </blockquote>
  ),
  code: ({ className, children }) => {
    const isInline = !className;
    if (isInline) {
      return (
        <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-sm">
          {children}
        </code>
      );
    }
    return (
      <code className={className}>
        {children}
      </code>
    );
  },
  pre: ({ children }) => (
    <pre className="my-4 overflow-x-auto rounded-lg bg-muted p-4 font-mono text-sm">
      {children}
    </pre>
  ),
  a: ({ href, children }) => (
    <a
      href={href}
      className="text-primary underline underline-offset-4 hover:text-primary/80"
      target={href?.startsWith("http") ? "_blank" : undefined}
      rel={href?.startsWith("http") ? "noopener noreferrer" : undefined}
    >
      {children}
    </a>
  ),
  table: ({ children }) => (
    <div className="my-4 overflow-x-auto">
      <table className="w-full border-collapse border border-border">
        {children}
      </table>
    </div>
  ),
  thead: ({ children }) => <thead className="bg-muted">{children}</thead>,
  th: ({ children }) => (
    <th className="border border-border px-4 py-2 text-left font-semibold">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-border px-4 py-2">{children}</td>
  ),
  hr: () => <hr className="my-6 border-border" />,
  img: ({ src, alt }) => (
    <img src={src} alt={alt} className="my-4 max-w-full rounded-lg" />
  ),
};
