'use client';

import posthog from 'posthog-js';
import { useEffect } from 'react';

// Named PostHog events for the Boundary pages, on top of autocapture, pageviews and page-leave scroll
// depth (all on by default, see instrumentation-client.ts). Every event is prefixed `boundary_` so
// the Boundary site's events filter apart from the BAML pages' on the same project.

/** Sends `boundary_<event>` to PostHog. A no-op when PostHog isn't initialized (no key, local dev). */
export function track(event: string, properties?: Record<string, unknown>) {
  try {
    posthog.capture(`boundary_${event}`, properties);
  } catch {}
}

/**
 * Sends `boundary_section_viewed` once per section per page load, when the section first reaches the
 * middle of the viewport. Sections are marked with `data-section="<name>"`; a marker may be a
 * `display: contents` wrapper, in which case its first child is what's watched.
 */
export function SectionViews() {
  useEffect(() => {
    const seen = new Set<string>();
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const marker = entry.target.closest<HTMLElement>('[data-section]');
          const section = marker?.dataset.section;
          io.unobserve(entry.target);
          if (!section || seen.has(section)) continue;
          seen.add(section);
          track('section_viewed', { order: seen.size, section });
        }
      },
      { rootMargin: '-40% 0px -40% 0px' },
    );
    for (const marker of document.querySelectorAll<HTMLElement>(
      '[data-section]',
    )) {
      const target =
        getComputedStyle(marker).display === 'contents'
          ? marker.firstElementChild
          : marker;
      if (target) io.observe(target);
    }
    return () => io.disconnect();
  }, []);
  return null;
}
