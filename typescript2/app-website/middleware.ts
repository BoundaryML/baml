import { type NextRequest, NextResponse } from 'next/server';
import { detectAgentMode } from '@/lib/agent-detect';

export function middleware(req: NextRequest) {
  if (req.nextUrl.searchParams.get('from') === 'toggle') {
    return NextResponse.next();
  }

  const result = detectAgentMode({
    accept: req.headers.get('accept'),
    userAgent: req.headers.get('user-agent'),
    searchParams: req.nextUrl.searchParams,
  });

  if (result.isAgent) {
    const url = req.nextUrl.clone();
    url.pathname = '/llms.txt';
    const res = NextResponse.rewrite(url);
    res.headers.set('Vary', 'Accept, User-Agent');
    res.headers.set('X-Agent-Mode', result.reason);
    return res;
  }

  const res = NextResponse.next();
  res.headers.append('Vary', 'Accept, User-Agent');
  return res;
}

export const config = {
  matcher: [
    '/((?!api|_next|_vercel|agent|llms\\.txt|relay-JkOu|favicon|robots\\.txt|sitemap|.*\\..*).*)',
  ],
};
