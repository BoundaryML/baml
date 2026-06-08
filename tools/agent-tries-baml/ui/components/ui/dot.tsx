import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';

const dot = cva(
  'inline-block size-[7px] mr-1.5 rounded-full bg-current align-baseline',
  {
    variants: {
      tone: {
        ok: 'text-success',
        warn: 'text-paper-warn',
        hot: 'text-destructive',
        link: 'text-link',
        mute: 'text-muted-foreground',
      },
    },
    defaultVariants: { tone: 'mute' },
  },
);

export type DotTone = VariantProps<typeof dot>['tone'];

/** Small status dot colored by tone (the legacy `.dot dot-*`). */
export function Dot({ tone, className }: { tone?: DotTone; className?: string }) {
  return <span className={cn(dot({ tone }), className)} />;
}
