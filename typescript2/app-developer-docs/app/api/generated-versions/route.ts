import { listDocumentVersionOptions } from '@/lib/generated-content/document-store';

export const dynamic = 'force-dynamic';
export const revalidate = 0;

export async function GET(request: Request) {
  const path = new URL(request.url).searchParams.get('path');
  if (!path || path.startsWith('/') || path.endsWith('/')) {
    return Response.json(
      { error: 'A valid stored document path is required.' },
      { status: 400 },
    );
  }
  return Response.json({ options: await listDocumentVersionOptions(path) });
}
