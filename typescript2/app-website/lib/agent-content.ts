import { readFile } from 'node:fs/promises';
import path from 'node:path';

export async function readAgentMarkdown(): Promise<string> {
  const [agentIntro, agentGuide] = await Promise.all([
    readFile(path.join(process.cwd(), 'content', 'agent.md'), 'utf-8'),
    readBamlAgentGuideMarkdown(),
  ]);

  return `${agentIntro.trim()}\n\n${agentGuide.trim()}\n`;
}

export function readBamlAgentGuideMarkdown(): Promise<string> {
  return readFile(
    path.join(process.cwd(), 'content', 'baml-agent-guide.md'),
    'utf-8',
  );
}
