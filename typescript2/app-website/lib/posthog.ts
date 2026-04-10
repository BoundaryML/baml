// app/posthog.js
import { PostHog } from 'posthog-node';
import { env } from './env';

export function postHogClient() {
  const posthogClient = new PostHog(env.NEXT_PUBLIC_POSTHOG_KEY, {
    flushAt: 1,
    flushInterval: 0,
    host: 'https://us.posthog.com',
  });
  return posthogClient;
}
