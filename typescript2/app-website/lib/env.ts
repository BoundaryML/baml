import { createEnv } from '@t3-oss/env-nextjs';
import { vercel } from '@t3-oss/env-nextjs/presets-zod';
import { z } from 'zod';

export const env = createEnv({
  /**
   * Specify your client-side environment variables schema here.
   * For them to be exposed to the client, prefix them with `NEXT_PUBLIC_`.
   */
  client: {
    NEXT_PUBLIC_APP_ENV: z
      .enum(['development', 'production', 'test'])
      .default('development'),
    NEXT_PUBLIC_POSTHOG_HOST: z.string().default('https://us.i.posthog.com'),
    NEXT_PUBLIC_POSTHOG_KEY: z.string().optional(),
  },

  extends: [vercel()],

  /**
   * Destructure all variables from `process.env` to make sure they aren't tree-shaken away.
   */
  runtimeEnv: {
    LUMA_API_KEY: process.env.LUMA_API_KEY,
    NEXT_PUBLIC_APP_ENV: process.env.NEXT_PUBLIC_APP_ENV,
    NEXT_PUBLIC_POSTHOG_HOST: process.env.NEXT_PUBLIC_POSTHOG_HOST,
    NEXT_PUBLIC_POSTHOG_KEY: process.env.NEXT_PUBLIC_POSTHOG_KEY,
    NODE_ENV: process.env.NODE_ENV,
    OPENAI_API_KEY: process.env.OPENAI_API_KEY,
  },
  server: {
    LUMA_API_KEY: z.string().optional(),
    OPENAI_API_KEY: z.string().optional(),
  },
  shared: {
    NODE_ENV: z
      .enum(['development', 'production', 'test'])
      .default('development'),
  },

  skipValidation:
    !!process.env.CI || process.env.npm_lifecycle_event === 'lint',
});
