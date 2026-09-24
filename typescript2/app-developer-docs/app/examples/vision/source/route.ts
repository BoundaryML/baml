import { loadProjectSnippet } from '@/lib/snippets/discovery';
import { selectProjectFiles } from '@/lib/snippets/selection';

export const dynamic = 'force-static';

export async function GET() {
  const project = await loadProjectSnippet('vision');
  const [file] = selectProjectFiles(project, 'baml_src/main.baml');
  return new Response(`${file.displaySource}\n`, {
    headers: {
      'Content-Disposition': 'attachment; filename="main.baml"',
      'Content-Type': 'text/plain; charset=utf-8',
    },
  });
}
