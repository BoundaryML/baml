import { createHash } from 'node:crypto';
import manifest from './annotation-manifest.json';
import type { CodeAnnotationId } from './annotation-specs';

export function getAnnotationMetadata(id: CodeAnnotationId, code: string) {
  const metadata = manifest[id];
  const sourceHash = createHash('sha256').update(code).digest('hex');
  if (!metadata || metadata.sourceHash !== sourceHash) {
    throw new Error(
      `Annotated image ${id} is out of date. Run pnpm docs:annotations:generate.`,
    );
  }
  return metadata;
}
