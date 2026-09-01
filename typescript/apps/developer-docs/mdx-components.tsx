import type { MDXComponents } from "mdx/types"
import defaultMdxComponents from "fumadocs-ui/mdx"

import { BamlExample } from "@/components/baml-example"
import { Quiz } from "@/components/quiz"
import { cn } from "@/lib/utils"

function getText(node: React.ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node)
  if (Array.isArray(node)) return node.map(getText).join("")
  if (node && typeof node === "object" && "props" in node) {
    return getText((node as React.ReactElement<{ children?: React.ReactNode }>).props.children)
  }
  return ""
}

function headingId(children: React.ReactNode) {
  return getText(children)
    .trim()
    .replace(/\s+/g, "-")
    .replace(/[?'\u2019]/g, "")
    .toLowerCase()
}

function heading(Tag: "h2" | "h3" | "h4") {
  return function Heading({ children, id, ...props }: React.ComponentProps<"h2">) {
    const resolvedId = id ?? headingId(children)
    return (
      <Tag id={resolvedId} {...props}>
        <a className="group no-underline" href={`#${resolvedId}`}>
          <span className="underline-offset-4 group-hover:underline">{children}</span>
          <span aria-hidden="true" className="ml-2 text-muted-foreground opacity-0 group-hover:opacity-100">#</span>
        </a>
      </Tag>
    )
  }
}

export const mdxComponents: MDXComponents = {
  ...defaultMdxComponents,
  BamlExample,
  Quiz,
  h2: heading("h2"),
  h3: heading("h3"),
  h4: heading("h4"),
  pre: ({ className, ...props }) => (
    <pre
      data-not-typeset
      className={cn("no-scrollbar min-w-0 overflow-x-auto px-4 py-3.5 outline-none", className)}
      {...props}
    />
  ),
}
