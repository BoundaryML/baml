import { type NextRequest, NextResponse } from 'next/server';
import { SESSION_COOKIE, signedSession, siteOrigin } from '@/lib/auth';

export async function GET(request: NextRequest) {
  const state = request.nextUrl.searchParams.get('state');
  const code = request.nextUrl.searchParams.get('code');
  const expected = request.cookies.get('atb2-oauth-state')?.value;
  let response: NextResponse;
  try {
    if (!state || state !== expected || !code || code.length > 1024)
      throw new Error('Invalid sign-in state');
    const exchange = await fetch(
      'https://github.com/login/oauth/access_token',
      {
        body: JSON.stringify({
          client_id: process.env.ATB2_GITHUB_CLIENT_ID,
          client_secret: process.env.ATB2_GITHUB_CLIENT_SECRET,
          code,
          redirect_uri: siteOrigin() + '/api/auth/github/callback',
        }),
        cache: 'no-store',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'application/json',
        },
        method: 'POST',
        redirect: 'error',
        signal: AbortSignal.timeout(15_000),
      },
    );
    const token = await exchange.json();
    if (!exchange.ok || typeof token.access_token !== 'string')
      throw new Error('GitHub sign-in failed');
    const options = {
      cache: 'no-store' as const,
      headers: {
        Accept: 'application/vnd.github+json',
        Authorization: `Bearer ${token.access_token}`,
      },
      redirect: 'error' as const,
      signal: AbortSignal.timeout(15_000),
    };
    const [userResponse, membershipResponse] = await Promise.all([
      fetch('https://api.github.com/user', options),
      fetch('https://api.github.com/user/memberships/orgs/BoundaryML', options),
    ]);
    if (!userResponse.ok || !membershipResponse.ok)
      throw new Error('BoundaryML membership required');
    const user = await userResponse.json();
    const membership = await membershipResponse.json();
    if (
      membership.state !== 'active' ||
      typeof user.login !== 'string' ||
      !/^[a-zA-Z0-9-]{1,39}$/.test(user.login)
    )
      throw new Error('BoundaryML membership required');
    response = NextResponse.redirect(siteOrigin());
    response.cookies.set(SESSION_COOKIE, signedSession(user.login), {
      httpOnly: true,
      maxAge: 3600,
      path: '/',
      sameSite: 'lax',
      secure: true,
    });
  } catch {
    response = NextResponse.json(
      {
        error:
          'Sign-in failed. An active BoundaryML membership and organization approval for the OAuth app are required.',
      },
      { status: 403 },
    );
  }
  response.cookies.set('atb2-oauth-state', '', {
    httpOnly: true,
    maxAge: 0,
    path: '/api/auth/github',
    sameSite: 'lax',
    secure: true,
  });
  response.headers.set('Cache-Control', 'no-store');
  return response;
}
