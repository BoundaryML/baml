import { cva } from 'class-variance-authority';

import { cn } from '@/lib/utils';

const pulse = cva('inline-block size-[9px] ml-1 align-middle rounded-full', {
  variants: {
    on: {
      true: 'bg-success animate-[atb-blink_1.6s_ease-in-out_infinite]',
      false: 'bg-muted-foreground',
    },
  },
  defaultVariants: { on: false },
});

/** Live-activity indicator dot; blinks when `on` (legacy `.pulse(.on)`). */
export function Pulse({ on = false, className }: { on?: boolean; className?: string }) {
  return <span className={cn(pulse({ on }), className)} />;
}
