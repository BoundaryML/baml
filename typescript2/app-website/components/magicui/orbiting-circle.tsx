'use client';

import {
  cubicBezier,
  type HTMLMotionProps,
  motion,
  useInView,
} from 'motion/react';
import React, { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

export interface OrbitingCirclesProps extends HTMLMotionProps<'div'> {
  className?: string;
  children?: React.ReactNode;
  reverse?: boolean;
  duration?: number;
  delay?: number;
  radius?: number;
  path?: boolean;
  iconSize?: number;
  speed?: number;
  index?: number;
  startAnimationDelay?: number;
  once?: boolean;
}

export function OrbitingCircles({
  className,
  children,
  reverse,
  duration = 20,
  radius = 160,
  path = true,
  iconSize = 30,
  speed = 1,
  index = 0,
  startAnimationDelay = 0,
  once = false,
  ...props
}: OrbitingCirclesProps) {
  const calculatedDuration = duration / speed;

  const ref = useRef(null);
  const isInView = useInView(ref, { once });
  const [shouldAnimate, setShouldAnimate] = useState(false);

  useEffect(() => {
    if (isInView) {
      setShouldAnimate(true);
    } else {
      setShouldAnimate(false);
    }
  }, [isInView]);
  return (
    <>
      {path && (
        <motion.div ref={ref}>
          {shouldAnimate && (
            <motion.div
              animate={{ opacity: 1, scale: 1 }}
              className="pointer-events-none absolute inset-0"
              initial={{ opacity: 0, scale: 0 }}
              style={{
                height: radius * 2,
                left: `calc(50% - ${radius}px)`,
                top: `calc(50% - ${radius}px)`,
                width: radius * 2,
              }}
              transition={{
                damping: 18,
                delay: index * 0.2 + startAnimationDelay,
                duration: 0.8,
                ease: [0.23, 1, 0.32, 1],
                mass: 1,
                stiffness: 120,
                type: 'spring',
              }}
            >
              <div
                className={cn(
                  'size-full rounded-full',
                  'border border-[0,0,0,0.07] dark:border-[rgba(249,250,251,0.07)]',
                  'bg-gradient-to-b from-[rgba(0,0,0,0.05)] from-0% via-[rgba(249,250,251,0.00)] via-54.76%',
                  'dark:bg-gradient-to-b dark:from-[rgba(249,250,251,0.03)] dark:from-0% dark:via-[rgba(249,250,251,0.00)] dark:via-54.76%',
                  className,
                )}
              />
            </motion.div>
          )}
        </motion.div>
      )}
      {shouldAnimate &&
        React.Children.map(children, (child, index) => {
          const angle = (360 / React.Children.count(children)) * index;
          return (
            <div
              className={cn(
                'absolute flex size-[var(--icon-size)] z-20 p-1 transform-gpu animate-orbit items-center justify-center rounded-full',
                { '[animation-direction:reverse]': reverse },
              )}
              style={
                {
                  '--angle': angle,
                  '--duration': calculatedDuration,
                  '--icon-size': `${iconSize}px`,
                  '--radius': radius * 0.98,
                } as React.CSSProperties
              }
            >
              <motion.div
                animate={{ opacity: 1, scale: 1 }}
                initial={{ opacity: 0, scale: 0 }}
                key={`orbit-child-${
                  // biome-ignore lint/suspicious/noArrayIndexKey: Array index is stable here since we're mapping over a fixed slice of children
                  index
                }`}
                transition={{
                  damping: 18,
                  delay: 0.6 + index * 0.2 + startAnimationDelay,
                  duration: 0.5,
                  ease: cubicBezier(0, 0, 0.58, 1),
                  mass: 1,
                  stiffness: 120,
                  type: 'spring',
                }}
                {...props}
              >
                {child}
              </motion.div>
            </div>
          );
        })}
    </>
  );
}
