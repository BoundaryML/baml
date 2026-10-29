/** biome-ignore-all lint/style/noNonNullAssertion: the ref is attached by this component before its effect runs */
'use client';

import { type ReactNode, useEffect, useRef, useState } from 'react';

/**
 * Fades its children up the first time they scroll into view. Content is visible by default
 * (no-JS, reduced motion, print); only once JS confirms it's below the fold does it hide and wait.
 */
export default function Reveal({
  children,
  className = '',
  delay = 0,
}: {
  children: ReactNode;
  className?: string;
  delay?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<'idle' | 'waiting' | 'shown'>('idle');

  useEffect(() => {
    const el = ref.current!;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    if (el.getBoundingClientRect().top < window.innerHeight * 0.9) return; // already on screen: leave it be
    setState('waiting');
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          setState('shown');
          io.disconnect();
        }
      },
      { rootMargin: '0px 0px -12% 0px' },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  return (
    <div
      className={`${className} transition-[opacity,transform] duration-700 ease-[cubic-bezier(0.22,1,0.36,1)] ${
        state === 'waiting'
          ? 'opacity-0 translate-y-3'
          : 'opacity-100 translate-y-0'
      }`}
      ref={ref}
      style={{ transitionDelay: state === 'shown' ? `${delay}ms` : '0ms' }}
    >
      {children}
    </div>
  );
}
