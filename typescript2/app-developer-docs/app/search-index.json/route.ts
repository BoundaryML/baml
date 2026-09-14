import { buildGeneratedSearchIndex } from '@/lib/generated-content/discovery';

export const dynamic = 'force-dynamic';
export const revalidate = 0;

export async function GET() {
  return Response.json(await buildGeneratedSearchIndex());
}
