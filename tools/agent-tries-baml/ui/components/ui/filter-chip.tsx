import { cva } from 'class-variance-authority';

import { cn } from '@/lib/utils';

const filterChip = cva(
  // preflight already makes buttons inherit the font family
  'appearance-none cursor-pointer rounded-full border bg-transparent ' +
    'px-2.5 py-[3px] text-[12.5px] hover:bg-muted',
  {
    variants: {
      active: {
        true: 'text-inherit bg-muted border-current',
        false: 'text-muted-foreground border-border',
      },
    },
    defaultVariants: { active: false },
  },
);

/** Toggleable pill chip used for outcome filters (legacy `.fchip`). */
export function FilterChip({
  active = false,
  onClick,
  className,
  children,
}: {
  active?: boolean;
  onClick?: () => void;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <button className={cn(filterChip({ active }), className)} onClick={onClick}>
      {children}
    </button>
  );
}

/** The wrapping row for filter chips (legacy `.filterbar`). */
export function FilterBar({ children }: { children: React.ReactNode }) {
  return <div className="flex flex-wrap gap-1.5 mb-2.5">{children}</div>;
}
