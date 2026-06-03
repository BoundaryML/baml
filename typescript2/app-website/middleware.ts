import { type NextRequest, NextResponse } from 'next/server';

// Agent detection is inlined here on purpose: the Edge middleware bundle must
// have no cross-module imports, which have proven fragile to resolve in the
// Vercel Edge runtime (unresolved import -> MIDDLEWARE_INVOCATION_FAILED).
const AGENT_UA_PATTERNS: RegExp[] = [
  /\bcurl\b/i,
  /\bwget\b/i,
  /\bhttpie\b/i,
  /\bpython-requests\b/i,
  /\bnode-fetch\b/i,
  /\bGo-http-client\b/i,
  /\bChatGPT-User\b/i,
  /\bGPTBot\b/i,
  /\bClaudeBot\b/i,
  /\bClaude-Web\b/i,
  /\banthropic-ai\b/i,
  /\bPerplexityBot\b/i,
  /\bcohere-ai\b/i,
  /\bBytespider\b/i,
];

const AGENT_PREFERRED_TYPES = ['text/markdown', 'text/plain', 'application/json'];

// Highest q-value the Accept header assigns to a given media type (0 if absent).
function preferredQ(accept: string, candidate: string): number {
  let best = 0;
  for (const raw of accept.split(',')) {
    const [type, ...params] = raw.trim().split(';');
    if ((type ?? '').trim().toLowerCase() !== candidate) continue;
    const qParam = params.find((p) => p.trim().startsWith('q='));
    const q = qParam ? Number.parseFloat(qParam.split('=')[1] ?? '1') : 1;
    best = Math.max(best, Number.isNaN(q) ? 1 : q);
  }
  return best;
}

type AgentResult = { isAgent: boolean; reason: string };

function detectAgent(
  accept: string | null,
  userAgent: string | null,
  params: URLSearchParams,
): AgentResult {
  if (params.get('format') === 'md' || params.get('agent') === '1') {
    return { isAgent: true, reason: 'query' };
  }

  if (accept) {
    const htmlQ = preferredQ(accept, 'text/html');
    const agentQ = Math.max(
      ...AGENT_PREFERRED_TYPES.map((t) => preferredQ(accept, t)),
    );
    if (agentQ > 0 && agentQ > htmlQ) {
      return { isAgent: true, reason: 'accept-header' };
    }
    if (htmlQ > 0 && htmlQ >= agentQ) {
      return { isAgent: false, reason: 'none' };
    }
  }

  if (userAgent && AGENT_UA_PATTERNS.some((re) => re.test(userAgent))) {
    return { isAgent: true, reason: 'user-agent' };
  }

  return { isAgent: false, reason: 'none' };
}

export function middleware(req: NextRequest) {
  try {
    const params = req.nextUrl.searchParams;
    if (params.get('from') === 'toggle') {
      return NextResponse.next();
    }

    const result = detectAgent(
      req.headers.get('accept'),
      req.headers.get('user-agent'),
      params,
    );

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
  } catch {
    // Agent detection must never take down the site — fail open.
    return NextResponse.next();
  }
}

export const config = {
  matcher: [
    '/((?!api|_next|_vercel|agent|llms\\.txt|relay-JkOu|favicon|robots\\.txt|sitemap|.*\\..*).*)',
  ],
};
