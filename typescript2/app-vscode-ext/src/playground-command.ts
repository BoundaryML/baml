import * as fs from 'node:fs';
import * as path from 'node:path';

export interface PlaygroundCommandOptions {
  platform: NodeJS.Platform;
  projectPath?: string;
  shell: string;
  wrapperPath: string;
}

function isPowerShellShell(platform: NodeJS.Platform, shell: string): boolean {
  if (platform !== 'win32') {
    return false;
  }
  const normalizedShell = shell.toLowerCase();
  return (
    normalizedShell.includes('powershell') ||
    /(^|[\\/])pwsh(?:\.exe)?$/.test(normalizedShell)
  );
}

function shellQuote(
  value: string,
  platform: NodeJS.Platform,
  shell: string,
): string {
  if (platform === 'win32') {
    if (isPowerShellShell(platform, shell)) {
      return `'${value.replace(/'/g, "''")}'`;
    }
    return `"${value.replace(/"/g, '""')}"`;
  }
  return `'${value.replace(/'/g, "'\\''")}'`;
}

export function playgroundCommandForPath({
  platform,
  projectPath,
  shell,
  wrapperPath,
}: PlaygroundCommandOptions): { command: string; cwd?: string } {
  const bin = `${isPowerShellShell(platform, shell) ? '& ' : ''}${shellQuote(wrapperPath, platform, shell)}`;
  if (!projectPath) {
    return { command: `${bin} playground` };
  }

  try {
    const stat = fs.statSync(projectPath);
    if (stat.isFile()) {
      return {
        command: `${bin} playground --file ${shellQuote(projectPath, platform, shell)}`,
        cwd: path.dirname(projectPath),
      };
    }
    if (stat.isDirectory()) {
      return {
        command: `${bin} playground --project ${shellQuote(projectPath, platform, shell)}`,
        cwd: projectPath,
      };
    }
  } catch {
    // Let the CLI report the path error while preserving project-path semantics.
  }

  return {
    command: `${bin} playground --project ${shellQuote(projectPath, platform, shell)}`,
  };
}
