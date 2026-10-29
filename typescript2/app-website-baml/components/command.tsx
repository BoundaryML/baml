'use client';

import { useState } from 'react';
import { sectionOf, track } from './track';

/**
 * A shell command to copy, in the install panel's frame (./install.tsx): a label row with Copy, then
 * the lines in mono with the same hanging indent as every other code pane.
 */
export default function Command({
  label,
  lines,
}: {
  label: string;
  lines: string[];
}) {
  const [copied, setCopied] = useState(false);

  const copy = (e: React.MouseEvent<HTMLButtonElement>) => {
    track('command_copied', {
      command: lines.join('\n'),
      label,
      section: sectionOf(e.currentTarget),
    });
    // A blocked clipboard (insecure context, denied permission) leaves the label as it was.
    navigator.clipboard?.writeText(lines.join('\n')).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  };

  return (
    <div className="flex flex-col rounded-xl bg-raised border border-rule overflow-hidden">
      <div className="flex items-center justify-between gap-4 pl-[18px] pr-2 py-2 border-b border-rule text-[13px] leading-4">
        <span className="text-muted">{label}</span>
        <button
          aria-live="polite"
          className="tap px-2.5 py-1.5 text-muted hover:text-ink transition-colors"
          onClick={copy}
          type="button"
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
      <pre className="code-pane px-[18px] pt-4 pb-[18px] font-mono text-[12.5px] leading-5 text-ink-2">
        {lines.map((l) => (
          <div className="code-line" key={l}>
            {l}
          </div>
        ))}
      </pre>
    </div>
  );
}
