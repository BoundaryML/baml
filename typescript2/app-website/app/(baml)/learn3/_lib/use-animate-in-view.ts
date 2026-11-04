'use client';

import { useCallback, useRef, useState } from 'react';

/**
 * Defer CSS animations until the element scrolls into view: apply the
 * returned `ref` and `holdClass` to the animated component's root. While
 * `anim-hold` is present, every descendant animation is paused at its
 * first frame (delays included); when the element intersects, the class
 * drops and the whole choreography plays from the start.
 *
 * IntersectionObserver wired through a ref callback — no useEffect, no
 * dependency. Fires once; replay buttons (key remounts) are unaffected.
 */
export function useAnimateInView(threshold = 0.3) {
  const [inView, setInView] = useState(false);
  const seenRef = useRef(false);

  const ref = useCallback(
    (el: Element | null) => {
      if (!el || seenRef.current) return;
      if (typeof IntersectionObserver === 'undefined') {
        seenRef.current = true;
        setInView(true);
        return;
      }
      const obs = new IntersectionObserver(
        (entries) => {
          if (entries.some((e) => e.isIntersecting)) {
            seenRef.current = true;
            setInView(true);
            obs.disconnect();
          }
        },
        { threshold },
      );
      obs.observe(el);
    },
    [threshold],
  );

  return { ref, holdClass: inView ? '' : ' anim-hold' };
}
