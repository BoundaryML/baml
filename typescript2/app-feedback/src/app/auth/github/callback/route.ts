import { cookies } from 'next/headers';
import { type NextRequest, NextResponse } from 'next/server';
import {
  cookieOptions,
  identity,
  STATE_COOKIE,
  setSession,
  siteOrigin,
  unseal,
} from '@/lib/approval-auth';

export async function GET(request: NextRequest) {
  const jar = await cookies();
  const stored = unseal(jar.get(STATE_COOKIE)?.value ?? '');
  jar.set(STATE_COOKIE, '', { ...cookieOptions, maxAge: 0 });
  const code = request.nextUrl.searchParams.get('code');
  if (
    !stored ||
    !code ||
    stored.state !== request.nextUrl.searchParams.get('state') ||
    typeof stored.verifier !== 'string' ||
    typeof stored.returnTo !== 'string' ||
    !/^\/(?:issues|proposals)\/[a-zA-Z0-9-]{1,100}$/.test(stored.returnTo)
  ) {
    return new NextResponse('Invalid or expired sign-in. Please start again.', {
      status: 400,
    });
  }
  try {
    const response = await fetch(
      'https://github.com/login/oauth/access_token',
      {
        body: JSON.stringify({
          client_id: process.env.FEEDBACK_GITHUB_CLIENT_ID,
          client_secret: process.env.FEEDBACK_GITHUB_CLIENT_SECRET,
          code,
          code_verifier: stored.verifier,
          redirect_uri: `${siteOrigin()}/auth/github/callback`,
        }),
        cache: 'no-store',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'application/json',
        },
        method: 'POST',
        redirect: 'error',
        signal: AbortSignal.timeout(10000),
      },
    );
    const result = await response.json();
    if (
      !response.ok ||
      typeof result.access_token !== 'string' ||
      !(await identity(result.access_token))
    ) {
      return new NextResponse('A verified GitHub account is required.', {
        status: 403,
      });
    }
    await setSession(result.access_token);
    return NextResponse.redirect(new URL(stored.returnTo, siteOrigin()));
  } catch {
    return new NextResponse(
      'Sign-in could not be verified. Please try again.',
      { status: 503 },
    );
  }
}
