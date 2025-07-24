import { cn } from '@baml/ui/lib/utils';

function Skeleton({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="skeleton"
      className={cn('animate-pulse rounded-md bg-primary/10', className)}
      {...props}
    />
  );
}

export { Skeleton };
