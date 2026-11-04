'use client';

import posthog from 'posthog-js';
import { useEffect } from 'react';
import { SECTION_ATTR } from './analytics';

// Named PostHog events for the homepage and blog, on top of autocapture, pageviews and page-leave
// scroll depth (all on by default, see instrumentation-client.ts). Every event is prefixed
// `boundary_`, like the rest of boundaryml.com (components/boundary/track.tsx), so the homepage's
// `boundary_section_viewed` keeps its name.

/** Sends `boundary_<event>` to PostHog. A no-op when PostHog isn't initialized (no key, local dev). */
export function track(event: string, properties?: Record<string, unknown>) {
  try {
    posthog.capture(`boundary_${event}`, properties);
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

/** The `section()` name of the region an element sits in, for events that need to say where. */
export function sectionOf(el: Element) {
  return (
    el.closest(`[${SECTION_ATTR}]`)?.getAttribute(SECTION_ATTR) ?? undefined
  );
}
