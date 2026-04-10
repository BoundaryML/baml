'use client';

import { useEffect, useRef, useState } from 'react';

export function useIntersectionTrigger(threshold = 0.5) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [isInView, setIsInView] = useState(false);
  const [hasTriggered, setHasTriggered] = useState(false);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        const active = entry.isIntersecting && entry.intersectionRatio >= threshold;
        setIsInView(active);
        if (active) setHasTriggered(true);
      },
      {
        threshold: [0.2, threshold, 0.9],
      },
    );

    observer.observe(element);
    return () => observer.disconnect();
  }, [threshold]);

  return { ref, hasTriggered, isInView };
}
