import posthog from 'posthog-js';

// PostHog, set up the same way as app-website's: autocapture, pageviews, page-leave scroll depth and
// exceptions. Events go through the /relay-JkOu rewrite in next.config.ts, so ad blockers don't drop
// them. Without NEXT_PUBLIC_POSTHOG_KEY (local dev) nothing is sent.
const key = process.env.NEXT_PUBLIC_POSTHOG_KEY;

if (key) {
  posthog.init(key, {
    api_host: process.env.NEXT_PUBLIC_POSTHOG_HOST ?? '/relay-JkOu',
    capture_exceptions: true,
    defaults: '2025-05-24',
    person_profiles: 'identified_only',
    ui_host: 'https://us.posthog.com',
  });
}
