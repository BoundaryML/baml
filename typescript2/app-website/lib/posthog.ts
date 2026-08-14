import { PostHog } from 'posthog-node';
import { env } from './env';

function postHogServerHost() {
  if (env.NEXT_PUBLIC_POSTHOG_HOST.startsWith('http')) {
    return env.NEXT_PUBLIC_POSTHOG_HOST;
  }

  return 'https://us.i.posthog.com';
}

export function postHogClient() {
  if (!env.NEXT_PUBLIC_POSTHOG_KEY) {
    return null;
  }

  const posthogClient = new PostHog(env.NEXT_PUBLIC_POSTHOG_KEY, {
    flushAt: 1,
    flushInterval: 0,
    host: postHogServerHost(),
  });
  return posthogClient;
}
