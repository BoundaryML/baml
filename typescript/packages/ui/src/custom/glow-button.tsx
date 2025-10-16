import { cn } from '@baml/ui/lib/utils';
import { Slot } from '@radix-ui/react-slot';
import { type VariantProps, cva } from 'class-variance-authority';
import { ArrowRight } from 'lucide-react';
import type * as React from 'react';

const glowButtonVariants = cva(
  'relative inline-flex items-center justify-center rounded-full text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50',
  {
    variants: {
      variant: {
        default: 'text-white',
        outline: 'text-white',
      },
      size: {
        default: 'h-12 px-6 py-2',
        sm: 'h-9 rounded-md px-3 text-xs',
        lg: 'h-14 px-8 text-base',
      },
      showArrow: {
        true: 'pr-12',
        false: '',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
      showArrow: false,
    },
  },
);

function GlowButton({
  className,
  variant,
  size,
  showArrow,
  asChild = false,
  children,
  ...props
}: React.ComponentProps<'button'> &
  VariantProps<typeof glowButtonVariants> & {
    asChild?: boolean;
  }) {
  const Comp = asChild ? Slot : 'button';

  return (
    <Comp
      data-slot="glow-button"
      className={cn(
        'group relative',
        glowButtonVariants({ variant, size, showArrow, className }),
      )}
      {...props}
    >
      <>
        {/* Glow effect container */}
        <div className="absolute -inset-0.5 bg-gradient-to-r from-green-400 via-blue-500 to-purple-600 rounded-full opacity-75 group-hover:opacity-100 transition duration-1000" />

        {/* Button content */}
        <div className="relative bg-black border border-transparent rounded-full px-6 py-2">
          {children}
          {showArrow && <ArrowRight className="absolute right-6 size-5" />}
        </div>
      </>
    </Comp>
  );
}

export { GlowButton, glowButtonVariants };
