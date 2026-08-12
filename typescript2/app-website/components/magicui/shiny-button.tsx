'use client';

import type { MotionProps } from 'motion/react';
import { motion } from 'motion/react';
import type React from 'react';
import { cn } from '@/lib/utils';

const animationProps = {
  animate: { '--x': '-100%', scale: 1 },
  initial: { '--x': '100%', scale: 0.8 },
  transition: {
    damping: 15,
    mass: 2,
    repeat: Number.POSITIVE_INFINITY,
    repeatDelay: 1,
    repeatType: 'loop',
    scale: {
      damping: 5,
      mass: 0.5,
      stiffness: 200,
      type: 'spring',
    },
    stiffness: 20,
    type: 'spring',
  },
  whileTap: { scale: 0.95 },
} as MotionProps;
interface ShinyButtonProps {
  text: React.ReactNode;
  className?: string;
}

export const ShinyButton = ({
  text = 'shiny-button',
  className,
}: ShinyButtonProps) => {
  return (
    <motion.button
      {...animationProps}
      className={cn(
        'relative rounded-lg px-6 py-2 font-medium backdrop-blur-xl transition-[box-shadow] duration-300 ease-in-out hover:shadow-2xs dark:bg-[radial-gradient(circle_at_50%_0%,hsl(var(--primary)/10%)_0%,transparent_60%)] dark:hover:shadow-[0_0_20px_hsl(var(--primary)/10%)]',
        className,
      )}
    >
      <span
        className="relative block h-full w-full text-sm uppercase tracking-wide text-[rgb(0,0,0,65%)] dark:font-light dark:text-[rgb(255,255,255,90%)]"
        style={{
          maskImage:
            'linear-gradient(-75deg,hsl(var(--primary)) calc(var(--x) + 20%),transparent calc(var(--x) + 30%),hsl(var(--primary)) calc(var(--x) + 100%))',
        }}
      >
        {text}
      </span>
      <span
        className="absolute inset-0 z-10 block rounded-[inherit] bg-[linear-gradient(-75deg,hsl(var(--primary)/10%)_calc(var(--x)+20%),hsl(var(--primary)/50%)_calc(var(--x)+25%),hsl(var(--primary)/10%)_calc(var(--x)+100%))] p-px"
        style={{
          mask: 'linear-gradient(rgb(0,0,0), rgb(0,0,0)) content-box,linear-gradient(rgb(0,0,0), rgb(0,0,0))',
          maskComposite: 'exclude',
        }}
      />
    </motion.button>
  );
};
