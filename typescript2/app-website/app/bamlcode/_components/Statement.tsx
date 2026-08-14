'use client';

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

/** Renders a problem's markdown statement. No worker/browser deps - SSR-safe. */
export function Statement({ markdown }: { markdown: string }) {
  return (
    <div className="bc-md">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{markdown}</ReactMarkdown>
    </div>
  );
}
