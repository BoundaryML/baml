import * as React from 'react';
import { ChevronDown } from 'lucide-react';

import { cn } from '../../lib/utils';

/** A native `<select>` styled to match `Input`. Native rather than a radix
 *  popover: option lists here are plain strings, and the native control keeps
 *  keyboard/scroll behavior for free inside webviews. `className` lands on the
 *  `<select>` itself (the kit's override contract — same as `input.tsx`); the
 *  wrapper div only positions the chevron. */
function Select({ className, ...props }: React.ComponentProps<'select'>) {
  return (
    <div className="relative inline-flex w-full">
      <select
        data-slot="select"
        className={cn(
          'h-7 w-full min-w-0 appearance-none rounded-md border border-vsc-input-border bg-vsc-input-bg text-vsc-input-fg pl-2 pr-6 text-xs shadow-xs outline-none transition-[color,box-shadow] disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50',
          'focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50',
          'aria-invalid:border-destructive aria-invalid:ring-destructive/20',
          className,
        )}
        {...props}
      />
      <ChevronDown
        size={12}
        className="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 text-muted-foreground"
      />
    </div>
  );
}

export { Select };
