import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';

/**
 * Bordered data grid (legacy `.runtable`). On mobile (<640px) the table
 * becomes a horizontally scrollable block via the `atb-table-mobile`
 * companion class (display/table semantics aren't utility-expressible).
 */
export function DataTable({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <table
      className={cn(
        'w-full border-collapse border border-border atb-table-mobile',
        className,
      )}
    >
      {children}
    </table>
  );
}

const th = cva(
  'border border-border bg-muted px-2.5 py-1.5 text-xs font-semibold text-muted-foreground',
  {
    variants: { align: { left: 'text-left', right: 'text-right' } },
    defaultVariants: { align: 'left' },
  },
);

/** Header cell; `align="right"` replaces the legacy `.r` class. */
export function Th({
  align,
  className,
  children,
}: {
  align?: VariantProps<typeof th>['align'];
  className?: string;
  children?: React.ReactNode;
}) {
  return <th className={cn(th({ align }), className)}>{children}</th>;
}

const td = cva(
  // long unbroken tokens must not widen the table past the viewport
  'border border-border px-2.5 py-2 text-[13.5px] align-top [overflow-wrap:anywhere]',
  {
    variants: {
      align: { left: '', right: 'text-right' },
      cell: {
        default: '',
        // the wide prompt column: single line with ellipsis (legacy td.task)
        task: 'max-w-0 w-[55%] whitespace-nowrap overflow-hidden text-ellipsis',
      },
    },
    defaultVariants: { align: 'left', cell: 'default' },
  },
);

/** Body cell; `align="right"` for numerics, `cell="task"` for the ellipsized prompt column. */
export function Td({
  align,
  cell,
  className,
  children,
}: {
  align?: VariantProps<typeof td>['align'];
  cell?: VariantProps<typeof td>['cell'];
  className?: string;
  children?: React.ReactNode;
}) {
  return <td className={cn(td({ align, cell }), className)}>{children}</td>;
}

/** Body row with the soft hover (legacy `.runrow`). */
export function Tr({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return <tr className={cn('hover:bg-muted', className)}>{children}</tr>;
}
