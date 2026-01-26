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
        draft: "border-transparent bg-slate-500 text-white",
        proposed: "border-transparent bg-blue-500 text-white",
        accepted: "border-transparent bg-green-500 text-white",
        implemented: "border-transparent bg-purple-500 text-white",
        rejected: "border-transparent bg-red-500 text-white",
        superseded: "border-transparent bg-orange-500 text-white",
        discussion: "border-transparent bg-gray-500 text-white",
        concern: "border-transparent bg-red-500 text-white",
        question: "border-transparent bg-blue-500 text-white",
        decision: "border-transparent bg-green-500 text-white",
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
