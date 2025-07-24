import fs from 'node:fs';
import os from 'node:os';
import type { ExtensionContext, OutputChannel } from 'vscode';
import { downloadedCliPath, ensureInstallPathExists } from './paths';
import type { CliVersion } from './types';

/**
 * Caches the bundled CLI to the install directory
 */
export async function cacheBundledCli(
  context: ExtensionContext,
  bundledCliPath: string,
  bundledVersion: string,
  outputChannel: OutputChannel,
): Promise<void> {
  // Skip caching on Windows for now
  if (os.platform() === 'win32') {
    return;
  }

  outputChannel.appendLine(
    `Checking if current bundled CLI (version: ${bundledVersion}) needs to be cached from ${bundledCliPath}...`,
  );

  const cliVersionMeta: CliVersion = {
    architecture: os.arch(),
    platform: os.platform(),
    version: bundledVersion,
  };

  const targetCachePath = downloadedCliPath(context, cliVersionMeta);

  try {
    // Check if already cached
    await fs.promises.access(targetCachePath);
    outputChannel.appendLine(
      `Bundled CLI version ${bundledVersion} is already cached at ${targetCachePath}. No copy needed.`,
    );
  } catch (e) {
    // Not cached, copy it
    outputChannel.appendLine(
      `Bundled CLI version ${bundledVersion} not found in cache. Attempting to cache from ${bundledCliPath} to ${targetCachePath}.`,
    );

    try {
      await ensureInstallPathExists(context);
      await fs.promises.copyFile(bundledCliPath, targetCachePath);
      outputChannel.appendLine(
        `Successfully copied bundled CLI to cache: ${targetCachePath}`,
      );

      // Set executable permissions
      try {
        await fs.promises.chmod(targetCachePath, 0o755);
        outputChannel.appendLine(
          `Ensured executable permissions for cached bundled CLI: ${targetCachePath}`,
        );
      } catch (chmodError: any) {
        outputChannel.appendLine(
          `ERROR: Failed to set executable permissions for cached bundled CLI ${targetCachePath}: ${chmodError.message}`,
        );
      }
    } catch (copyError: any) {
      outputChannel.appendLine(
        `ERROR: Failed to cache bundled CLI version ${bundledVersion} from ${bundledCliPath} to ${targetCachePath}: ${copyError.message}`,
      );
    }
  }
}
