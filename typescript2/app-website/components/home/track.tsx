'use client';

import posthog, { type CaptureOptions } from 'posthog-js';
import { useEffect } from 'react';
import { SECTION_ATTR } from './analytics';
import { BOOK } from './links';

// Named PostHog events for the homepage and blog, on top of autocapture, pageviews and page-leave
// scroll depth (all on by default, see instrumentation-client.ts). Every event is prefixed
// `boundary_`, so the homepage's `boundary_section_viewed` keeps the name the earlier homepage used.

/** Sends `boundary_<event>` to PostHog. A no-op when PostHog isn't initialized (no key, local dev). */
export function track(
  event: string,
  properties?: Record<string, unknown>,
  options?: CaptureOptions,
) {
  try {
    posthog.capture(`boundary_${event}`, properties, options);
  } catch {}
}

/**
 * Sends `boundary_section_viewed` once per section per page load, when the section first reaches
 * the middle of the viewport. Sections are the elements marked with `section()` from ./analytics.ts.
 */
export function SectionViews() {
  useEffect(() => {
    const seen = new Set<string>();
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          io.unobserve(entry.target);
          const name = entry.target.getAttribute(SECTION_ATTR);
          if (!name || seen.has(name)) continue;
          seen.add(name);
          track('section_viewed', { order: seen.size, section: name });
        }
      },
      { rootMargin: '-40% 0px -40% 0px' },
    );
    for (const el of document.querySelectorAll(`[${SECTION_ATTR}]`))
      io.observe(el);
    return () => io.disconnect();
  }, []);
  return null;
}

/**
 * Sends `boundary_cta_clicked` when a visitor clicks a link to the booking page (BOOK), from any
 * section: the nav, the hero, the booking band, or the footer. It carries the link's `section` and
 * `label`. The link opens the booking page in the same tab, so the event goes out by sendBeacon,
 * which the browser still delivers after the page unloads. A middle click (auxclick) counts too.
 */
export function CtaClicks() {
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      if (e.type === 'auxclick' && e.button !== 1) return;
      const link =
        e.target instanceof Element ? e.target.closest('a[href]') : null;
      if (!(link instanceof HTMLAnchorElement) || link.href !== BOOK) return;
      track(
        'cta_clicked',
        {
          href: BOOK,
          label: link.textContent?.trim(),
          section: sectionOf(link),
        },
        { send_instantly: true, transport: 'sendBeacon' },
      );
    };
    document.addEventListener('click', onClick);
    document.addEventListener('auxclick', onClick);
    return () => {
      document.removeEventListener('click', onClick);
      document.removeEventListener('auxclick', onClick);
    };
  }, []);
  return null;
}

/** The `section()` name of the region an element sits in, for events that need to say where. */
export function sectionOf(el: Element) {
  return (
    el.closest(`[${SECTION_ATTR}]`)?.getAttribute(SECTION_ATTR) ?? undefined
  );
}
