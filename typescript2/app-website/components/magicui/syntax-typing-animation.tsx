'use client';

import { type MotionProps, motion } from 'motion/react';
import { useTheme } from 'next-themes';
import { useEffect, useRef, useState } from 'react';
import { codeToHtml } from 'shiki';
import { cn } from '@/lib/utils';

interface SyntaxTypingAnimationProps extends MotionProps {
  code: string;
  language?: string;
  className?: string;
  duration?: number;
  delay?: number;
  as?: React.ElementType;
}

export const SyntaxTypingAnimation = ({
  code,
  language = 'typescript',
  className,
  duration = 60,
  delay = 0,
  as: Component = 'span',
  ...props
}: SyntaxTypingAnimationProps) => {
  const MotionComponent = motion.create(Component, {
    forwardMotionProps: true,
  });

  const [displayedText, setDisplayedText] = useState<string>('');
  const [highlightedCode, setHighlightedCode] = useState<string>('');
  const [started, setStarted] = useState(false);
  const elementRef = useRef<HTMLElement | null>(null);
  const theme = useTheme();

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
      if (i < code.length) {
        setDisplayedText(code.substring(0, i + 1));
        i++;
      } else {
        clearInterval(typingEffect);
      }
    }, duration);

    return () => {
      clearInterval(typingEffect);
    };
  }, [code, duration, started]);

  // Update highlighted code when displayed text changes
  useEffect(() => {
    if (displayedText) {
      const highlightPartialCode = async () => {
        try {
          const highlighted = await codeToHtml(displayedText, {
            lang: language,
            theme: theme.theme === 'dark' ? 'github-dark' : 'github-light',
          });
          setHighlightedCode(highlighted);
        } catch (error) {
          // Fallback to plain text if highlighting fails
          setHighlightedCode(displayedText);
        }
      };
      highlightPartialCode();
    } else {
      setHighlightedCode('');
    }
  }, [displayedText, language, theme.theme]);

  return (
    <MotionComponent
      className={cn('text-sm font-normal tracking-tight', className)}
      ref={elementRef}
      {...props}
    >
      {highlightedCode ? (
        <div
          className="syntax-highlighted-code bg-none"
          // biome-ignore lint/security/noDangerouslySetInnerHtml: pre-processed code highlighting
          dangerouslySetInnerHTML={{
            __html: highlightedCode,
          }}
        />
      ) : (
        <span>{displayedText}</span>
      )}
    </MotionComponent>
  );
};
