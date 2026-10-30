'use client';

import { type ReactNode, useEffect, useRef } from 'react';

/** Sets data-seen on itself the first time it scrolls into view, so CSS can play its entrance once. */
export default function Seen({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          el.dataset.seen = 'true';
          io.disconnect();
        }
      },
      { threshold: 0.2 },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);
  return (
    <div className={className} data-seen="false" ref={ref}>
      {children}
    </div>
  );
}
