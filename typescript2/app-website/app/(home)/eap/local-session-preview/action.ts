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
      status: 'success',
      signupType: 'onboarding',
      session: {
        url: '/eap/local-session-preview?state=join',
        startAt: new Date(now + startsIn * 60_000).toISOString(),
        endAt: new Date(now + (startsIn + 60) * 60_000).toISOString(),
        zoomUrl: startsIn <= 15 ? 'https://zoom.us/test' : undefined,
      },
    };
  }
  return {
    status: 'error',
    message: 'This is a local preview. No registration was sent.',
  };
}
