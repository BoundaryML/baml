import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center rounded-md border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-primary text-primary-foreground shadow hover:bg-primary/80",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground hover:bg-secondary/80",
        destructive:
          "border-transparent bg-destructive text-destructive-foreground shadow hover:bg-destructive/80",
        outline: "text-foreground",
        open: "border-transparent bg-blue-500 text-white",
        in_progress: "border-transparent bg-amber-500 text-white",
        merged: "border-transparent bg-purple-500 text-white",
        shipped: "border-transparent bg-green-600 text-white",
        deferred: "border-transparent bg-slate-500 text-white",
        rejected: "border-transparent bg-red-500 text-white",
        trivial: "border-transparent bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-200",
        easy: "border-transparent bg-sky-100 text-sky-800 dark:bg-sky-900/50 dark:text-sky-200",
        medium: "border-transparent bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-200",
        hard: "border-transparent bg-rose-100 text-rose-800 dark:bg-rose-900/50 dark:text-rose-200",
        subsystem: "bg-muted text-muted-foreground font-medium",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  );
}

export { Badge, badgeVariants };
