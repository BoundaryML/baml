'use client';

import { useId, useState } from 'react';
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
  const defId = useId();
  return (
    <Popover onOpenChange={setOpen} open={open}>
      <PopoverTrigger asChild>
        <button
          aria-describedby={defId}
          className="cursor-help bg-transparent p-0 text-inherit underline decoration-current/50 decoration-dotted underline-offset-[0.2em] [font:inherit] focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2"
          onBlur={() => setOpen(false)}
          // A tap fires mouseenter and focus before click, so both have already opened it.
          // preventDefault stops Radix's trigger from toggling it shut again.
          onClick={(event) => {
            event.preventDefault();
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onMouseEnter={() => setOpen(true)}
          onMouseLeave={() => setOpen(false)}
          type="button"
        >
          {children}
        </button>
      </PopoverTrigger>
      {/* The popover only mounts while open; screen readers read the definition from here. */}
      <span className="sr-only" id={defId}>
        {def}
      </span>
      <PopoverContent
        aria-hidden
        className="w-auto max-w-80 border-[var(--rule)] bg-[var(--raised)] px-4 py-3 text-[14px] leading-[22px] text-[var(--ink-2)]"
        // Focus never enters the popover, so there is nothing to restore. Radix's default
        // focuses the trigger on close, which would fire onFocus and reopen it.
        onCloseAutoFocus={(event) => event.preventDefault()}
        onOpenAutoFocus={(event) => event.preventDefault()}
        side="top"
      >
        {def}
      </PopoverContent>
    </Popover>
  );
}
