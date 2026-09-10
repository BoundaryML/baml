import { cookies } from 'next/headers';
import { type NextRequest, NextResponse } from 'next/server';
import {
  challenge,
  cookieOptions,
  issuePath,
  nonce,
  proposalPath,
  STATE_COOKIE,
  seal,
  siteOrigin,
} from '@/lib/approval-auth';

export async function GET(request: NextRequest) {
  try {
    const id = request.nextUrl.searchParams.get('proposal') ?? '';
    const issue = request.nextUrl.searchParams.get('issue');
    const returnTo = issue !== null ? issuePath(issue) : proposalPath(id);
    const clientId = process.env.FEEDBACK_GITHUB_CLIENT_ID;
    if (!clientId) throw new Error('OAuth not configured');
    const state = nonce();
    const verifier = nonce();
    (await cookies()).set(
      STATE_COOKIE,
      seal({ expires: Date.now() + 600000, returnTo, state, verifier }),
      { ...cookieOptions, maxAge: 600 },
    );
    const url = new URL('https://github.com/login/oauth/authorize');
    url.search = new URLSearchParams({
      client_id: clientId,
      code_challenge: challenge(verifier),
      code_challenge_method: 'S256',
      redirect_uri: `${siteOrigin()}/auth/github/callback`,
      scope: 'read:user',
      state,
    }).toString();
    return NextResponse.redirect(url);
  } catch {
    return new NextResponse(
      'Website approval sign-in is unavailable. You can approve the proposal in Slack.',
      { status: 503 },
    );
  }
}
