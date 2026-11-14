'use server';

import type { submitSignup as liveSignup } from '@/app/actions/signup';

export async function submitSignup(
  _previous: Parameters<typeof liveSignup>[0],
  form: FormData,
): ReturnType<typeof liveSignup> {
  const eventId = form.get('eventId');
  const startsIn =
    eventId === 'preview-live'
      ? -10
      : eventId === 'preview-soon'
        ? 10
        : eventId === 'preview-later'
          ? 10_080
          : null;
  if (
    process.env.NODE_ENV === 'development' &&
    form.get('signupType') === 'onboarding' &&
    startsIn !== null
  ) {
    const now = Date.now();
    return {
      session: {
        endAt: new Date(now + (startsIn + 60) * 60_000).toISOString(),
        startAt: new Date(now + startsIn * 60_000).toISOString(),
        url: '/eap/local-session-preview?state=join',
        zoomUrl: startsIn <= 15 ? 'https://zoom.us/test' : undefined,
      },
      signupType: 'onboarding',
      status: 'success',
    };
  }
  return {
    message: 'This is a local preview. No registration was sent.',
    status: 'error',
  };
}
