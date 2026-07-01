import { readFile } from 'node:fs/promises';
import path from 'node:path';

export async function readAgentMarkdown(): Promise<string> {
  const content = await readFile(
    path.join(process.cwd(), 'content', 'agent.md'),
    'utf-8',
  );
  return `${content.trim()}\n`;
}

export function readBamlAgentGuideMarkdown(): Promise<string> {
  return readFile(
    path.join(process.cwd(), 'content', 'baml-agent-guide.md'),
    'utf-8',
  );
}
