'use client';

import posthog, { type CaptureOptions } from 'posthog-js';
import { useEffect } from 'react';
import { SECTION_ATTR } from './analytics';
import { SIGNUP } from './links';

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

/** Tracks clicks on links to the early access page. */
export function CtaClicks() {
  useEffect(() => {
    const signupUrl = new URL(SIGNUP, window.location.origin).href;
    const onClick = (e: MouseEvent) => {
      if (e.type === 'auxclick' && e.button !== 1) return;
      const link =
        e.target instanceof Element ? e.target.closest('a[href]') : null;
      if (!(link instanceof HTMLAnchorElement) || link.href !== signupUrl)
        return;
      track(
        'cta_clicked',
        {
          href: `${link.pathname}${link.hash}`,
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
