import { cva } from 'class-variance-authority';

import { cn } from '@/lib/utils';

const tab = cva(
  'appearance-none cursor-pointer bg-transparent border border-transparent border-b-0 ' +
    '-mb-px rounded-t-md px-3.5 py-1.5 text-sm',
  {
    variants: {
      active: {
        true: 'font-semibold text-inherit border-border bg-muted',
        false: 'text-muted-foreground hover:text-inherit',
      },
    },
    defaultVariants: { active: false },
  },
);

/** The tab strip container (legacy `.tabbar`); wrap in a section with `mt-6` for `.tabsec`. */
export function TabBar({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex gap-1 border-b border-border mb-3" role="tablist">
      {children}
    </div>
  );
}

/** One tab button; pass `count` for the muted number badge (legacy `.tab`/`.tabnum`). */
export function Tab({
  active = false,
  onClick,
  count,
  className,
  children,
}: {
  active?: boolean;
  onClick?: () => void;
  count?: number;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <button
      role="tab"
      aria-selected={active}
      className={cn(tab({ active }), className)}
      onClick={onClick}
    >
      {children}
      {count != null ? <TabNum>{count}</TabNum> : null}
    </button>
  );
}

/** The muted count badge inside a tab (legacy `.tabnum`). */
export function TabNum({ children }: { children: React.ReactNode }) {
  return (
    <span className="ml-1 text-[11.5px] font-normal text-muted-foreground">
      {children}
    </span>
  );
}
