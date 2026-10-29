'use client';

import { useSearchParams } from 'next/navigation';
import { Suspense, useCallback, useMemo, useRef, useState } from 'react';
import { cn } from '@/lib/utils';
import { getSlides } from './slides';

function clamp(n: number, lo: number, hi: number) {
  return Math.max(lo, Math.min(n, hi));
}

/**
 * The slide-deck shell. Navigation state lives in client state (source of truth);
 * the URL `?page=` is kept in sync with `history.replaceState` so paging never
 * triggers a server re-render (the server already rendered every slide once).
 *
 * Deliberately no `useEffect`:
 *  - initial page is seeded from `useSearchParams()` during render,
 *  - focus is grabbed via a ref callback,
 *  - keyboard nav listens on the container (events bubble up from buttons too).
 */
export function Deck() {
  // The whole deck is client-side: build slide nodes once (only the current one
  // mounts) so the BAML editors/playground live here without a server boundary.
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
      // Sync the URL here in the event handler — NOT inside the setIdx updater.
      // Next's App Router monkey-patches `history.replaceState` to dispatch a
      // router action; running it inside the state updater fires that dispatch
      // during React's render phase ("Cannot update Router while rendering Deck").
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
      className="l2-deck"
      aria-roledescription="carousel"
    >
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
        {/* key={idx} replays the per-slide entrance animation.
            The Suspense boundary is load-bearing for keyboard nav: slides
            that suspend (BamlCode's `use(getLearnHighlighter())` on first
            render) would otherwise suspend the whole deck, hiding the
            focused .l2-deck container — focus falls to <body> and arrow
            keys die. With the boundary, only the slide content is held
            back and the deck keeps focus. */}
        <div key={idx} className="l2-slide">
          <Suspense fallback={null}>{slides[idx]?.node}</Suspense>
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
