export const AGENT_UA_PATTERNS: RegExp[] = [
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

const AGENT_PREFERRED_TYPES = [
  'text/markdown',
  'text/plain',
  'application/json',
];

type AcceptEntry = { type: string; q: number };

function parseAccept(header: string | null | undefined): AcceptEntry[] {
  if (!header) return [];
  return header
    .split(',')
    .map((raw) => {
      const [type, ...params] = raw.trim().split(';');
      const qParam = params.find((p) => p.trim().startsWith('q='));
      const q = qParam ? Number.parseFloat(qParam.split('=')[1] ?? '1') : 1;
      return { type: (type ?? '').trim().toLowerCase(), q: Number.isNaN(q) ? 1 : q };
    })
    .filter((e) => e.type.length > 0);
}

function preferredQ(entries: AcceptEntry[], candidate: string): number {
  let best = 0;
  for (const e of entries) {
    if (e.type === candidate) best = Math.max(best, e.q);
  }
  return best;
}

export type AgentDetectInput = {
  accept?: string | null;
  userAgent?: string | null;
  searchParams?: URLSearchParams | Record<string, string | undefined> | null;
};

export type AgentDetectResult = {
  isAgent: boolean;
  reason: 'query' | 'user-agent' | 'accept-header' | 'none';
};

function getParam(
  source: AgentDetectInput['searchParams'],
  key: string,
): string | undefined {
  if (!source) return undefined;
  if (source instanceof URLSearchParams) return source.get(key) ?? undefined;
  return source[key];
}

export function detectAgentMode(input: AgentDetectInput): AgentDetectResult {
  const format = getParam(input.searchParams, 'format');
  const agentParam = getParam(input.searchParams, 'agent');
  if (format === 'md' || agentParam === '1') {
    return { isAgent: true, reason: 'query' };
  }

  const entries = parseAccept(input.accept);
  const htmlQ = preferredQ(entries, 'text/html');
  const agentQ = entries.length
    ? Math.max(
        ...AGENT_PREFERRED_TYPES.map((t) => preferredQ(entries, t)),
      )
    : 0;

  if (agentQ > 0 && agentQ > htmlQ) {
    return { isAgent: true, reason: 'accept-header' };
  }

  if (htmlQ > 0 && htmlQ >= agentQ) {
    return { isAgent: false, reason: 'none' };
  }

  const ua = input.userAgent ?? '';
  if (ua && AGENT_UA_PATTERNS.some((re) => re.test(ua))) {
    return { isAgent: true, reason: 'user-agent' };
  }

  return { isAgent: false, reason: 'none' };
}
