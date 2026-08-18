import * as fs from 'node:fs';
import * as path from 'node:path';

export interface PlaygroundCommandOptions {
  platform: NodeJS.Platform;
  projectPath?: string;
  shell: string;
  wrapperPath: string;
}

export interface WindowsTerminalProfile {
  path?: string | string[];
  source?: string;
}

export function shellForDefaultWindowsProfile(
  profileName: string | null | undefined,
  profiles: Record<string, WindowsTerminalProfile | null> | undefined,
): string {
  if (!profileName) {
    return 'PowerShell';
  }
  const profile = profiles?.[profileName];
  if (profile?.source) {
    return profile.source;
  }
  if (profile?.path) {
    return Array.isArray(profile.path) ? profile.path[0] : profile.path;
  }
  return profileName;
}

type ShellKind = 'cmd' | 'posix' | 'powershell';

function shellKind(platform: NodeJS.Platform, shell: string): ShellKind {
  if (platform !== 'win32') {
    return 'posix';
  }
  const normalizedShell = shell.toLowerCase();
  if (
    normalizedShell.includes('powershell') ||
    /(^|[\\/])pwsh(?:\.exe)?$/.test(normalizedShell)
  ) {
    return 'powershell';
  }
  if (
    normalizedShell === 'command prompt' ||
    /(^|[\\/])cmd(?:\.exe)?$/.test(normalizedShell)
  ) {
    return 'cmd';
  }
  return 'posix';
}

function shellQuote(
  value: string,
  platform: NodeJS.Platform,
  shell: string,
): string {
  const kind = shellKind(platform, shell);
  if (kind === 'powershell') {
    return `'${value.replace(/'/g, "''")}'`;
  }
  if (kind === 'cmd') {
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
  const bin = `${shellKind(platform, shell) === 'powershell' ? '& ' : ''}${shellQuote(wrapperPath, platform, shell)}`;
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
