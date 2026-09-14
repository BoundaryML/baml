import { type NextRequest, NextResponse } from 'next/server';

const ROUTES = new Map<string, { alternate: string; slug: string }>([
  ['/', { alternate: '/index.md', slug: 'index' }],
  ['/index.md', { alternate: '/index.md', slug: 'index' }],
  ['/quickstart', { alternate: '/quickstart.md', slug: 'quickstart' }],
  ['/quickstart.md', { alternate: '/quickstart.md', slug: 'quickstart' }],
  ['/explore', { alternate: '/explore.md', slug: 'explore' }],
  ['/explore.md', { alternate: '/explore.md', slug: 'explore' }],
  ['/pricing', { alternate: '/pricing.md', slug: 'pricing' }],
  ['/pricing.md', { alternate: '/pricing.md', slug: 'pricing' }],
]);

function acceptsMarkdown(request: NextRequest) {
  if (request.nextUrl.pathname.endsWith('.md')) return true;

  const accept = request.headers.get('accept')?.toLowerCase().trim() ?? '';
  if (accept.includes('text/markdown')) return true;
  if (accept.includes('text/html')) return false;

  // Plain curl and most generic HTTP clients send */*. For these curated
  // landing routes, Markdown is the most useful representation.
  return accept === '' || accept === '*/*';
}

export function middleware(request: NextRequest) {
  const route = ROUTES.get(request.nextUrl.pathname);
  if (!route) return NextResponse.next();

  if (acceptsMarkdown(request)) {
    const target = new URL('/api/markdown-page', request.url);
    const requestHeaders = new Headers(request.headers);
    requestHeaders.set('x-baml-markdown-page', route.slug);
    return NextResponse.rewrite(target, {
      headers: { Vary: 'Accept' },
      request: { headers: requestHeaders },
    });
  }

  const response = NextResponse.next();
  response.headers.set('Vary', 'Accept');
  // Next reserves and rewrites Vary for its React Server Component router.
  // Prevent shared caches from mixing this HTML representation with Markdown.
  response.headers.set('Cache-Control', 'private, no-store');
  response.headers.set('CDN-Cache-Control', 'no-store');
  response.headers.set('Vercel-CDN-Cache-Control', 'no-store');
  response.headers.set(
    'Link',
    `<${new URL(route.alternate, request.url)}>; rel="alternate"; type="text/markdown"`,
  );
  return response;
}

export const config = {
  matcher: ['/((?!api|_next/static|_next/image|favicon.ico).*)'],
};
