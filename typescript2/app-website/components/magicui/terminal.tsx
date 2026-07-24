'use client';

import { FileIcon, PlayIcon } from 'lucide-react';
import { type MotionProps, motion } from 'motion/react';
import { type PropsWithChildren, useEffect, useRef, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { Button } from '../ui/button';

interface AnimatedSpanProps extends MotionProps {
  delay?: number;
  className?: string;
}

export const AnimatedSpan = ({
  children,
  delay = 0,
  className,
  ...props
}: PropsWithChildren<AnimatedSpanProps>) => (
  <motion.div
    animate={{ opacity: 1, y: 0 }}
    className={cn('grid text-sm font-normal tracking-tight', className)}
    initial={{ opacity: 0, y: -5 }}
    transition={{ delay: delay / 1000, duration: 0.3 }}
    {...props}
  >
    {children}
  </motion.div>
);

interface TypingAnimationProps extends MotionProps {
  children: string;
  className?: string;
  duration?: number;
  delay?: number;
  as?: React.ElementType;
}

export const TypingAnimation = ({
  children,
  className,
  duration = 60,
  delay = 0,
  as: Component = 'span',
  ...props
}: TypingAnimationProps) => {
  if (typeof children !== 'string') {
    throw new Error('TypingAnimation: children must be a string. Received:');
  }

  const MotionComponent = motion.create(Component, {
    forwardMotionProps: true,
  });

  const [displayedText, setDisplayedText] = useState<string>('');
  const [started, setStarted] = useState(false);
  const elementRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const startTimeout = setTimeout(() => {
      setStarted(true);
    }, delay);
    return () => clearTimeout(startTimeout);
  }, [delay]);

  useEffect(() => {
    if (!started) return;

    let i = 0;
    const typingEffect = setInterval(() => {
      if (i < children.length) {
        setDisplayedText(children.substring(0, i + 1));
        i++;
      } else {
        clearInterval(typingEffect);
      }
    }, duration);

    return () => {
      clearInterval(typingEffect);
    };
  }, [children, duration, started]);

  return (
    <MotionComponent
      className={cn('text-sm font-normal tracking-tight', className)}
      ref={elementRef}
      {...props}
    >
      {displayedText}
    </MotionComponent>
  );
};

interface TerminalProps {
  children: React.ReactNode;
  className?: string;
  filename?: string;
}

export const Terminal = ({ children, className, filename }: TerminalProps) => {
  return (
    <div
      className={cn(
        'relative z-0 h-full max-h-[400px] w-full max-w-lg rounded-xl border border-border bg-background',
        className,
      )}
    >
      <div className="terminal-header relative z-10 flex flex-col gap-y-2 border-b border-border px-4 py-2">
        <div className="flex flex-row gap-x-2 items-center">
          <div className="h-2 w-2 rounded-full bg-red-500" />
          <div className="h-2 w-2 rounded-full bg-yellow-500" />
          <div className="h-2 w-2 rounded-full bg-green-500" />
          {filename && (
            <Badge className="px-3 py-1.5 text-sm" variant="outline">
              <FileIcon className="mr-2 size-4" />
              {filename}
            </Badge>
          )}
          {/* <div className="ml-auto">
            <Button
              asChild
              className="flex items-center justify-center gap-x-2"
              size="sm"
              variant="outline"
            >
              <a href="https://www.google.com">
                <PlayIcon className="size-4" />
                Try Now
              </a>
            </Button>
          </div> */}
        </div>
      </div>
      <pre className="terminal-content relative z-0 py-4 px-6 md:px-4 md:py-4">
        <code className="grid gap-y-1 overflow-auto">{children}</code>
      </pre>
    </div>
  );
};
