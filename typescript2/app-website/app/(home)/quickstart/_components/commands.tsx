'use client';

import { useState } from 'react';

// A terminal-style block of shell commands for the quickstart's steps. Each command sits on its own
// line with an optional note above it and a copy button beside it. The note is never copied.

export type Command = { cmd: string; note?: string };

/** One command with its note and a button that copies just the command. */
function Line({ cmd, note }: Command) {
  const [copied, setCopied] = useState(false);
  // The button says "Copied" only once the clipboard write has succeeded. If the clipboard is
  // unavailable or the write is refused, the button keeps saying "Copy".
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 1400);
  };
  return (
    <div className="group">
      {note && <p className="text-muted"># {note}</p>}
      <div className="flex items-start gap-3">
        <p className="min-w-0 flex-1 break-all">
          <span aria-hidden className="select-none text-clover">
            ${' '}
          </span>
          <span className="text-ink">{cmd}</span>
        </p>
        <button
          aria-label={`Copy ${cmd}`}
          className="focus-ring shrink-0 rounded-md px-2 py-0.5 text-[12px] text-muted transition-colors hover:text-ink-2"
          onClick={copy}
          type="button"
        >
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
    </div>
  );
}

/** A block of commands, separated by a little space when they carry notes. */
export function Commands({ commands }: { commands: Command[] }) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-rule bg-raised px-4 py-3.5 font-mono text-[13.5px] leading-[1.6]">
      {commands.map((c) => (
        <Line key={c.cmd} {...c} />
      ))}
    </div>
  );
}
