import React, {
  type ComponentPropsWithoutRef,
  type CSSProperties,
} from 'react';

import { cn } from '@/lib/utils';

interface RippleProps extends ComponentPropsWithoutRef<'div'> {
  mainCircleSize?: number;
  mainCircleOpacity?: number;
  numCircles?: number;
}

export const Ripple = React.memo(function Ripple({
  mainCircleSize = 210,
  mainCircleOpacity = 0.24,
  numCircles = 8,
  className,
  ...props
}: RippleProps) {
  return (
    <div
      className={cn(
        'pointer-events-none absolute inset-0 select-none [mask-image:linear-gradient(to_bottom,white,transparent)]',
        className,
      )}
      {...props}
    >
      {Array.from({ length: numCircles }, (_, i) => {
        const size = mainCircleSize + i * 70;
        const opacity = mainCircleOpacity - i * 0.03;
        const animationDelay = `${i * 0.06}s`;
        const borderStyle = 'solid';

        return (
          <div
            className={
              'absolute animate-ripple rounded-full border bg-foreground/25 shadow-xl'
            }
            key={`ripple-${i}`}
            style={
              {
                '--i': i,
                animationDelay,
                borderColor: 'var(--foreground)',
                borderStyle,
                borderWidth: '1px',
                height: `${size}px`,
                left: '50%',
                opacity,
                top: '50%',
                transform: 'translate(-50%, -50%) scale(1)',
                width: `${size}px`,
              } as CSSProperties
            }
          />
        );
      })}
    </div>
  );
});

Ripple.displayName = 'Ripple';
