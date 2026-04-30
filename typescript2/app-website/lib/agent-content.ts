import { readFile } from 'node:fs/promises';
import path from 'node:path';

export function readAgentMarkdown(): Promise<string> {
  return readFile(path.join(process.cwd(), 'content', 'agent.md'), 'utf-8');
}
