'use client';

import { useInView, useMotionValue, useSpring } from 'motion/react';
import { useEffect, useRef } from 'react';
import { cn } from '@/lib/utils';

export function NumberTicker({
  value,
  direction = 'up',
  delay = 0,
  className,
  formatter = (num: number) => Intl.NumberFormat('en-US').format(num),
}: {
  value: number;
  direction?: 'up' | 'down';
  className?: string;
  delay?: number; // delay in s
  formatter?: (num: number) => string;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  const motionValue = useMotionValue(direction === 'down' ? value : 0);
  const springValue = useSpring(motionValue, {
    damping: 60,
    stiffness: 100,
  });
  const isInView = useInView(ref, { margin: '0px', once: true });

  useEffect(() => {
    isInView &&
      setTimeout(() => {
        motionValue.set(direction === 'down' ? 0 : value);
      }, delay * 1000);
  }, [motionValue, isInView, delay, value, direction]);

  useEffect(
    () =>
      springValue.on('change', (latest) => {
        if (ref.current) {
          ref.current.textContent = formatter(Number(latest.toFixed(0)));
        }
      }),
    [springValue, formatter],
  );

  return (
    <span
      className={cn('inline-block tabular-nums tracking-wider', className)}
      ref={ref}
    />
  );
}
