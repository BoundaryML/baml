import { cn } from '@/lib/utils';

import { Dot, type DotTone } from './dot';

/** Inline status chip: a tone dot + label, never wrapping (legacy `.chip`). */
export function Chip({
  tone,
  className,
  children,
}: {
  tone?: DotTone;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span className={cn('inline-flex items-center whitespace-nowrap', className)}>
      <Dot tone={tone} />
      {children}
    </span>
  );
}
