import { readAgentMarkdown } from '@/lib/agent-content';

export const dynamic = 'force-static';

export async function GET() {
  const body = await readAgentMarkdown();
  return new Response(body, {
    headers: {
      'Content-Type': 'text/markdown; charset=utf-8',
      Vary: 'Accept, User-Agent',
      'Cache-Control': 'public, max-age=60, s-maxage=300',
    },
  });
}
