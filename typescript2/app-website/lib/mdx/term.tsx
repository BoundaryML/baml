'use client';

import { useState } from 'react';
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover';

// A word in a post with a short definition: dotted underline, the definition on hover, focus or tap.
// A popover rather than a tooltip, because tooltips never open on touch screens.
export function MDXTerm({
  children,
  def,
}: {
  children: React.ReactNode;
  def: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover onOpenChange={setOpen} open={open}>
      <PopoverTrigger asChild>
        <button
          className="cursor-help bg-transparent p-0 text-inherit underline decoration-current/50 decoration-dotted underline-offset-[0.2em] [font:inherit] focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2"
          onBlur={() => setOpen(false)}
          onFocus={() => setOpen(true)}
          onMouseEnter={() => setOpen(true)}
          onMouseLeave={() => setOpen(false)}
          type="button"
        >
          {children}
        </button>
      </PopoverTrigger>
      <PopoverContent
        className="w-auto max-w-80 border-[var(--rule)] bg-[var(--raised)] px-4 py-3 text-[14px] leading-[22px] text-[var(--ink-2)]"
        onOpenAutoFocus={(event) => event.preventDefault()}
        side="top"
      >
        {def}
      </PopoverContent>
    </Popover>
  );
}
