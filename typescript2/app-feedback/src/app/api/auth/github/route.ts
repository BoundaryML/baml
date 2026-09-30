import { randomBytes } from 'node:crypto';
import { type NextRequest, NextResponse } from 'next/server';
import { siteOrigin } from '@/lib/auth';

export async function GET(request: NextRequest) {
  // Set the state cookie on the same host that receives the OAuth callback.
  const origin = siteOrigin();
  if (request.nextUrl.origin !== origin) {
    const response = NextResponse.redirect(origin + '/api/auth/github');
    response.headers.set('Cache-Control', 'no-store');
    return response;
  }
  if (
    !process.env.ATB2_GITHUB_CLIENT_ID ||
    !process.env.ATB2_GITHUB_CLIENT_SECRET ||
    !process.env.ATB2_UI_SESSION_SECRET
  ) {
    return NextResponse.json(
      { error: 'GitHub sign-in has not been configured.' },
      { status: 503 },
    );
  }
  const state = randomBytes(32).toString('hex');
  const url = new URL('https://github.com/login/oauth/authorize');
  url.search = new URLSearchParams({
    client_id: process.env.ATB2_GITHUB_CLIENT_ID,
    redirect_uri: siteOrigin() + '/api/auth/github/callback',
    scope: 'read:org',
    state,
  }).toString();
  const response = NextResponse.redirect(url);
  response.cookies.set('atb2-oauth-state', state, {
    httpOnly: true,
    maxAge: 600,
    path: '/api/auth/github',
    sameSite: 'lax',
    secure: true,
  });
  response.headers.set('Cache-Control', 'no-store');
  return response;
}
