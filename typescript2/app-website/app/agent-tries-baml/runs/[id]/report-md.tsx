'use client';

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

/**
 * Renders a trophy's `reportMd` markdown into the editorial `.md-body` style,
 * with GitHub-flavored markdown (tables, fenced code, lists) support.
 * @param children - the markdown source to render
 */
export default function ReportMd({ children }: { children: string }) {
  return (
    <div className="md-body">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{children}</ReactMarkdown>
    </div>
  );
}
