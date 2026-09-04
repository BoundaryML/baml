import { buildGeneratedSearchIndex } from '@/lib/generated-content/discovery';

export const dynamic = 'force-static';

export async function GET() {
  return Response.json(await buildGeneratedSearchIndex());
}
