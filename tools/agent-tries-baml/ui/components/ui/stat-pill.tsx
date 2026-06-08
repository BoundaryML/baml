import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';

const statPill = cva('font-mono text-xs uppercase tracking-[0.04em]', {
  variants: {
    tone: {
      default: '', // most statuses (queued/running/open/...) have no color — inherit
      success: 'text-success',
      destructive: 'text-destructive',
      mute: 'text-muted-foreground',
      link: 'text-link',
    },
  },
  defaultVariants: { tone: 'default' },
});

export type StatPillTone = VariantProps<typeof statPill>['tone'];

// raw status string → tone, mirroring the legacy .statpill.* color rules exactly
const STATUS_TONE: Record<string, StatPillTone> = {
  passed: 'success',
  completed: 'success',
  success: 'success',
  failed: 'destructive',
  error: 'destructive',
  blocked: 'destructive',
  partial: 'mute',
  timeout: 'mute',
  cursor: 'link',
  warm: 'success',
  cold: 'link',
};

/**
 * Maps a raw status/outcome string to its StatPill tone.
 * Unknown statuses render uncolored (inherit), matching the legacy CSS.
 */
export function toneFromStatus(status: string): StatPillTone {
  return STATUS_TONE[status] ?? 'default';
}

/**
 * Status pill: small uppercase mono label, colored by tone.
 * Pass `status` to derive the tone from a raw status string, or set `tone` directly.
 */
export function StatPill({
  tone,
  status,
  className,
  children,
}: {
  tone?: StatPillTone;
  status?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        statPill({ tone: tone ?? (status ? toneFromStatus(status) : 'default') }),
        className,
      )}
    >
      {children}
    </span>
  );
}
