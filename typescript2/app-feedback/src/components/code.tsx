import React from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { bamlTokens } from "@/lib/code";

const colors = {
  comment: "text-slate-500 dark:text-slate-400",
  string: "text-emerald-700 dark:text-emerald-300",
  keyword: "text-purple-700 dark:text-purple-300",
  number: "text-orange-700 dark:text-orange-300",
  type: "text-blue-700 dark:text-blue-300",
};
const codeStyle = "overflow-x-auto rounded-md bg-muted/50 p-4 text-xs leading-relaxed";
function ColoredCode({ text }: { text: string }) {
  return bamlTokens(text).map((token, i) => <span key={i} className={token.kind ? colors[token.kind] : undefined}>{token.text}</span>);
}

export function CodeBlock({ text, language = "baml" }: { text: string; language?: string }) {
  return <pre className={codeStyle}><code>{language === "baml" || language.endsWith(".baml") ? <ColoredCode text={text} /> : text}</code></pre>;
}

/** Render untrusted Markdown without HTML execution or remote image requests. */
export function FormattedText({ text }: { text: string }) {
  return <div className="max-w-[75ch] space-y-4 break-words text-sm leading-7">
    <Markdown skipHtml remarkPlugins={[remarkGfm]} components={{
      h1: ({ children }) => <h3 className="mt-6 text-lg font-semibold">{children}</h3>,
      h2: ({ children }) => <h3 className="mt-6 text-base font-semibold">{children}</h3>,
      h3: ({ children }) => <h4 className="mt-5 font-semibold">{children}</h4>,
      p: ({ children }) => <p className="my-3">{children}</p>,
      ul: ({ children }) => <ul className="my-3 list-disc space-y-1 pl-6">{children}</ul>,
      ol: ({ children }) => <ol className="my-3 list-decimal space-y-1 pl-6">{children}</ol>,
      blockquote: ({ children }) => <blockquote className="border-l-2 pl-4 text-muted-foreground">{children}</blockquote>,
      pre: ({ children }) => <pre className={codeStyle}>{children}</pre>,
      code: ({ children, className }) => {
        const source = String(children);
        const language = /language-(\S+)/.exec(className ?? "")?.[1];
        const block = Boolean(language) || source.endsWith("\n");
        return <code className={block ? "font-mono" : "rounded bg-muted px-1 py-0.5 font-mono text-[0.9em]"}>
          {language === "baml" || (block && !language) ? <ColoredCode text={source} /> : children}
        </code>;
      },
      a: ({ href, children }) => <a href={href} className="underline underline-offset-2" rel="noreferrer">{children}</a>,
      img: ({ alt }) => <span className="text-muted-foreground">{alt || "Image"}</span>,
      table: ({ children }) => <div className="overflow-x-auto"><table className="w-full border-collapse text-left text-xs">{children}</table></div>,
      th: ({ children }) => <th className="border p-2 font-semibold">{children}</th>,
      td: ({ children }) => <td className="border p-2">{children}</td>,
    }}>{text}</Markdown>
  </div>;
}
