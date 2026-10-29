'use client';

import { useSearchParams } from 'next/navigation';
import { useCallback, useMemo, useRef, useState } from 'react';
import { cn } from '@/lib/utils';
import { getSlides } from './slides';

function clamp(n: number, lo: number, hi: number) {
  return Math.max(lo, Math.min(n, hi));
}

/**
 * The /learn3 deck shell — same mechanics as /learn2 (client-side slides,
 * `?page=` synced via history.replaceState, no useEffect), pointed at this
 * deck's slide registry and reusing the l2-* styles.
 */
export function Deck() {
  const slides = useMemo(() => getSlides(), []);
  const total = slides.length;
  const params = useSearchParams();
  const seeded = (Number.parseInt(params.get('page') ?? '1', 10) || 1) - 1;
  const [idx, setIdx] = useState(() => clamp(seeded, 0, total - 1));
  const grabbedFocus = useRef(false);

  const go = useCallback(
    (n: number) => {
      const next = clamp(n, 0, total - 1);
      setIdx(next);
      // Sync the URL in the event handler — not inside the state updater
      // (Next patches history.replaceState to dispatch a router action).
      if (typeof window !== 'undefined') {
        window.history.replaceState(null, '', `?page=${next + 1}`);
      }
    },
    [total],
  );

  const focusRef = useCallback((el: HTMLDivElement | null) => {
    if (el && !grabbedFocus.current) {
      grabbedFocus.current = true;
      el.focus({ preventScroll: true });
    }
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowRight':
      case 'PageDown':
        e.preventDefault();
        go(idx + 1);
        break;
      case 'ArrowLeft':
      case 'PageUp':
        e.preventDefault();
        go(idx - 1);
        break;
      case 'Home':
        e.preventDefault();
        go(0);
        break;
      case 'End':
        e.preventDefault();
        go(total - 1);
        break;
      default:
        break;
    }
  };

  const current = slides[idx];

  return (
    // biome-ignore lint/a11y/noNoninteractiveTabindex: deck is a keyboard-driven widget
    <div
      ref={focusRef}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="l2-deck l3-deck"
      aria-roledescription="carousel"
    >
      <div className="l3-progress" aria-hidden>
        <span style={{ width: `${((idx + 1) / total) * 100}%` }} />
      </div>
      <header className="l2-deck-head">
        <a href="/" className="l2-wordmark font-mono">
          BAML <span>· Learn</span>
        </a>
        <span className="l2-section-tag font-mono">{current?.section}</span>
        <span className="l2-counter font-mono">
          {String(idx + 1).padStart(2, '0')} / {String(total).padStart(2, '0')}
        </span>
      </header>

      <main className="l2-stage">
        {/* key={idx} replays the per-slide entrance animation */}
        <div key={idx} className="l2-slide">
          {slides[idx]?.node}
        </div>
      </main>

      <footer className="l2-deck-foot">
        <button
          type="button"
          className="l2-nav-btn"
          onClick={() => go(idx - 1)}
          disabled={idx === 0}
          aria-label="Previous slide"
        >
          ←
        </button>
        <div className="l2-dots">
          {slides.map((m, i) => (
            <button
              key={m.id}
              type="button"
              className={cn('l2-dot', i === idx && 'l2-dot--on')}
              onClick={() => go(i)}
              title={`${i + 1}. ${m.title}`}
              aria-label={`Go to slide ${i + 1}: ${m.title}`}
              aria-current={i === idx}
            />
          ))}
        </div>
        <button
          type="button"
          className="l2-nav-btn"
          onClick={() => go(idx + 1)}
          disabled={idx === total - 1}
          aria-label="Next slide"
        >
          →
        </button>
      </footer>
    </div>
  );
}
