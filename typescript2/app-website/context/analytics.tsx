'use client';

import { Analytics } from '@vercel/analytics/next';
import { SpeedInsights } from '@vercel/speed-insights/next';
import type { PropsWithChildren } from 'react';
import { env } from '@/lib/env';
import { WebVitals } from '../lib/web-vitals';

// Ensure PostHog is initialized on the client

const isProduction = env.NEXT_PUBLIC_APP_ENV === 'production';

export function AnalyticsProvider(props: PropsWithChildren) {
  return (
    <>
      {isProduction && (
        <>
          <WebVitals />
          {props.children}
          <Analytics />
          <SpeedInsights />
        </>
      )}
      {!isProduction && props.children}
    </>
  );
}
