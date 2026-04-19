import { readFile } from 'node:fs/promises';
import path from 'node:path';

let cached: Promise<string> | null = null;

export function readAgentMarkdown(): Promise<string> {
  if (!cached) {
    cached = readFile(
      path.join(process.cwd(), 'content', 'agent.md'),
      'utf-8',
    );
  }
  return cached;
}
