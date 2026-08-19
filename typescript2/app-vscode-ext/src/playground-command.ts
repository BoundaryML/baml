import * as fs from 'node:fs';
import * as path from 'node:path';

export interface PlaygroundCommandOptions {
  projectPath?: string;
  wrapperPath: string;
}

export function playgroundCommandForPath({
  projectPath,
  wrapperPath,
}: PlaygroundCommandOptions): {
  args: string[];
  cwd?: string;
  executable: string;
} {
  if (!projectPath) {
    return { args: ['playground'], executable: wrapperPath };
  }

  try {
    const stat = fs.statSync(projectPath);
    if (stat.isFile()) {
      return {
        args: ['playground', '--file', projectPath],
        cwd: path.dirname(projectPath),
        executable: wrapperPath,
      };
    }
    if (stat.isDirectory()) {
      return {
        args: ['playground', '--project', projectPath],
        cwd: projectPath,
        executable: wrapperPath,
      };
    }
  } catch {
    // Let the CLI report the path error while preserving project-path semantics.
  }

  return {
    args: ['playground', '--project', projectPath],
    executable: wrapperPath,
  };
}
