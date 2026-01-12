"use client";

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { markdownComponents } from "@/lib/markdown";

interface BepContentProps {
  content: string;
}

export function BepContent({ content }: BepContentProps) {
  return (
    <div className="prose-wrapper">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={markdownComponents}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
