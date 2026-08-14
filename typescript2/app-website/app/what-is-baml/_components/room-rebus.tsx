'use client';

import { useEffect, useRef, useState } from 'react';

// ROOM with an elephant inside each O. Two things have to happen at the right
// moment, so this is a client component rather than a CSS-only animation:
//   1. the elephants walk in when the word is actually on screen, not on page
//      load (the rebus sits far down the page)
//   2. if you sit with it for a few seconds without getting it, the caption
//      appears. Hover does the same thing immediately, but touch has no hover,
//      so the timer is what covers mobile.
export function RoomRebus() {
  const ref = useRef<HTMLParagraphElement>(null);
  // `armed` only flips once JS runs, so with no JS the elephants render plainly
  // instead of being hidden forever by a class that never gets removed.
  const [armed, setArmed] = useState(false);
  const [seen, setSeen] = useState(false);
  const [explained, setExplained] = useState(false);

  useEffect(() => {
    setArmed(true);
    const el = ref.current;
    if (!el) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setSeen(true);
          observer.disconnect();
        }
      },
      { threshold: 0.6 },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!seen) return;
    const timer = setTimeout(() => setExplained(true), 1500);
    return () => clearTimeout(timer);
  }, [seen]);

  const className = [
    'wib-room',
    armed ? 'is-armed' : '',
    seen ? 'is-seen' : '',
    explained ? 'is-explained' : '',
  ]
    .filter(Boolean)
    .join(' ');

  return (
    <p
      aria-label="Elephant in the room"
      className={className}
      ref={ref}
      role="img"
    >
      <span aria-hidden="true">
        R
        <span className="wib-room-o">
          O<span className="wib-room-eleph">🐘</span>
        </span>
        <span className="wib-room-o">
          O<span className="wib-room-eleph">🐘</span>
        </span>
        M
      </span>
      <span className="wib-room-cap">elephants in the room</span>
    </p>
  );
}
