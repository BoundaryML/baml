import { cva, type VariantProps } from 'class-variance-authority';

import { cn } from '@/lib/utils';

/**
 * Editorial data grid: no cell borders, no header fill — just hairline rules
 * between rows and tabular numerals, so dense data reads like a printed table.
 * On mobile (<640px) it becomes a horizontally scrollable block via the
 * `atb-table-mobile` companion class (display/table semantics aren't
 * utility-expressible).
 */
export function DataTable({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <table className={cn('w-full border-collapse atb-table-mobile', className)}>
      {children}
    </table>
  );
}

const th = cva(
  'border-b border-border pb-1.5 pr-3 text-[12.5px] font-medium text-muted-foreground last:pr-0',
  {
    variants: { align: { left: 'text-left', right: 'text-right' } },
    defaultVariants: { align: 'left' },
  },
);

/** Header cell; `align="right"` for numeric columns. */
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
  'border-b border-border py-2 pr-3 text-[13.5px] align-top tabular-nums [overflow-wrap:anywhere] last:pr-0',
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

/** Body row with the soft hover; the last row drops its bottom rule. */
export function Tr({
  className,
  children,
}: {
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <tr className={cn('hover:bg-muted [&:last-child>td]:border-b-0', className)}>
      {children}
    </tr>
  );
}
