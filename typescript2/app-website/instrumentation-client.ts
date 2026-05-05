'use client';

import posthog from 'posthog-js';
import { env } from '@/lib/env';

if (env.NEXT_PUBLIC_POSTHOG_KEY) {
  posthog.init(env.NEXT_PUBLIC_POSTHOG_KEY, {
    api_host: env.NEXT_PUBLIC_POSTHOG_HOST,
    capture_exceptions: true,
    defaults: '2025-05-24',
    person_profiles: 'identified_only',
    ui_host: 'https://us.posthog.com',
  });
}
