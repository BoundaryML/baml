import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { annotationSpecs, type CodeAnnotationId } from './annotation-specs';
import { loadProjectSnippet } from './discovery';
import { selectProjectFiles } from './selection';

export interface AnnotationSource {
  id: CodeAnnotationId;
  code: string;
  language: 'baml' | 'typescript' | 'rust';
}

export async function loadAnnotationSources(): Promise<AnnotationSource[]> {
  const chapter = await readFile(
    resolve('content/baml/book/errors.mdx'),
    'utf8',
  );
  const examples = [
    ...chapter.matchAll(
      /<CodeExample language="(typescript|rust)" annotation="([^"]+)">\s*```(?:typescript|rust)\n([\s\S]*?)\n```\s*<\/CodeExample>/g,
    ),
  ];
  const sources: AnnotationSource[] = [];
  for (const id of Object.keys(annotationSpecs)) {
    // SAFETY: Object.keys enumerates only the keys in the static annotation catalog.
    const annotationId = id as CodeAnnotationId;
    const example = examples.find((match) => match[2] === id);
    if (example) {
      sources.push({
        code: example[3],
        id: annotationId,
        language: example[1] === 'rust' ? 'rust' : 'typescript',
      });
      continue;
    }
    const projectTags = [
      ...chapter.matchAll(
        /<BamlProject id="([^"]+)" file="([^"]+)" regions=\{\["([^"]+)"\]\} annotation="([^"]+)" \/>/g,
      ),
    ];
    const projectTag = projectTags.find((match) => match[4] === id);
    if (!projectTag) throw new Error(`No source for annotation ${id}`);
    const project = await loadProjectSnippet(projectTag[1]);
    const [file] = selectProjectFiles(project, projectTag[2], [projectTag[3]]);
    sources.push({
      code: file.displaySource,
      id: annotationId,
      language: 'baml',
    });
  }
  return sources;
}
