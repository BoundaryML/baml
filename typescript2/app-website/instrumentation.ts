import type { Instrumentation } from 'next';

export function register() {
  // No-op for initialization
}

export const onRequestError: Instrumentation.onRequestError = async (
  err,
  request,
) => {
  if (process.env.NEXT_RUNTIME === 'nodejs') {
    const { postHogClient } = await import('./lib/posthog');
    let distinctId = null;

    if (request.headers.cookie) {
      const cookieString = request.headers.cookie as string;
      const postHogCookieMatch = cookieString.match(
        /ph_phc_.*?_posthog=([^;]+)/,
      );

      if (postHogCookieMatch?.[1]) {
        try {
          const decodedCookie = decodeURIComponent(postHogCookieMatch[1]);
          const postHogData = JSON.parse(decodedCookie);
          distinctId = postHogData.distinct_id;
        } catch (e) {
          console.error('Error parsing PostHog cookie:', e);
        }
      }
    }

    postHogClient().captureException(err, distinctId || undefined);
  }
};
