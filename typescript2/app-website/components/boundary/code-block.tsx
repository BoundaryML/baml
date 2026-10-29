'use client';

import { useState } from 'react';
import { track } from './track';

/**
 * A code panel in article prose. Dark in both themes, like the code windows on the homepage
 * (./baml.tsx and ./loop.tsx): same panel colour, same hairline, same mono tab row.
 * Long lines scroll sideways rather than wrapping, so indentation stays readable. In dark mode the
 * panel lifts a shade off the page, which the homepage gets for free by sitting on a painting.
 */
export default function CodeBlock({
  lang,
  filename,
  code,
}: {
  lang: string;
  filename?: string;
  code: string;
}) {
  const [copied, setCopied] = useState(false);

  // Clipboard access can be refused (an insecure origin, or a denied permission). Nothing useful to
  // show for that, so the label just doesn't change.
  const copy = () => {
    track('code_copied', { filename, lang });
    navigator.clipboard?.writeText(code).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  };

  return (
    <div className="my-7 rounded-xl overflow-hidden bg-[#0F0F0F] dark:bg-[#17191b] border border-white/10">
      <div className="flex items-center justify-between gap-5 px-5 py-3.5 border-b border-[#2A2E28] font-mono text-xs">
        <span className="min-w-0 truncate text-white">{filename ?? lang}</span>
        <button
          className="focus-ring shrink-0 cursor-pointer text-[#6E756A] hover:text-white transition-colors"
          onClick={copy}
          type="button"
        >
          {copied ? 'copied' : 'copy'}
          <span className="sr-only"> {filename ?? lang} to clipboard</span>
        </button>
      </div>
      <pre className="px-5 py-5 overflow-x-auto font-mono text-[12px] leading-5 md:text-[13px] md:leading-[22px] text-[#D8DDD3]">
        <code data-lang={lang}>{code}</code>
      </pre>
    </div>
  );
}
