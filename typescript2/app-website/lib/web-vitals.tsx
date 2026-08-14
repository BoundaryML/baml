'use client';

import { useReportWebVitals } from 'next/web-vitals';
import { event } from 'nextjs-google-analytics';
import posthog from 'posthog-js';

export function WebVitals() {
  useReportWebVitals((metric) => {
    // Guard against missing init
    try {
      posthog.capture(metric.name, metric);
    } catch {}
    event(metric.name, {
      category:
        metric.name === 'web-vital' ? 'Web Vitals' : 'Next.js custom metric',
      // values must be integers
      label: metric.id,
      // id unique to current page load
      nonInteraction: true,
      value: Math.round(
        metric.name === 'CLS' ? metric.value * 1000 : metric.value,
      ), // avoids affecting bounce rate.
    });
  });

  return <div />;
}
