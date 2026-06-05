'use client';

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

/**
 * Renders a changelog entry's markdown body into the editorial `.md-body` style,
 * with GitHub-flavored markdown (tables, fenced code, lists) support. Code blocks
 * are plain (no syntax highlighting) — the Fly UI ships no shiki pipeline.
 * @param children - the markdown source to render
 */
export default function ChangelogBody({ children }: { children: string }) {
  return (
    <div className="md-body">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{children}</ReactMarkdown>
    </div>
  );
}
