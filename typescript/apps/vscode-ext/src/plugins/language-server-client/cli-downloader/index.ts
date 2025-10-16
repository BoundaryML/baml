import os from 'node:os';
import axios from 'axios';
import semver from 'semver';
import {
  type ExtensionContext,
  type OutputChannel,
  ProgressLocation,
  window,
} from 'vscode';

import { BackoffManager } from './backoff';
import { getBundledCliPath } from './bundled-cli';
import { cacheBundledCli } from './cache';
import { downloadAndVerifyCli } from './download';
import {
  checkIfDownloadedCliExists,
  downloadedCliPath,
  ensureExecutablePermissions,
  ensureInstallPathExists,
} from './paths';
// Import all modules
import type { CliVersion } from './types';

// Re-export types and commonly used functions
export type { CliVersion };
export { downloadedCliPath, checkIfDownloadedCliExists } from './paths';
export { getReleaseArchitecture, getReleasePlatform } from './platform';

// Singleton backoff manager
const backoffManager = new BackoffManager();

/**
 * Downloads and extracts the CLI binary for a specific version.
 */
export async function downloadCli(
  context: ExtensionContext,
  cliVersion: CliVersion,
  bamlOutputChannel: OutputChannel,
): Promise<string | null> {
  const installPath = await ensureInstallPathExists(context);

  try {
    const result = await downloadAndVerifyCli(
      cliVersion,
      installPath,
      bamlOutputChannel,
    );

    if (!result) {
      return null;
    }

    bamlOutputChannel.appendLine(
      `Successfully downloaded BAML CLI v${cliVersion.version}`,
    );

    // Reset backoff state on success
    backoffManager.clearBackoff(cliVersion.version);
    bamlOutputChannel.appendLine(
      `Reset download backoff state for version ${cliVersion.version}.`,
    );

    return result.targetFullPath;
  } catch (error: any) {
    bamlOutputChannel.appendLine(
      `ERROR: Failed during download process for BAML CLI v${cliVersion.version}: ${error.message}`,
    );

    if (error.stack) {
      bamlOutputChannel.appendLine(`Stack trace: ${error.stack}`);
    }

    // Log specific Axios errors
    if (axios.isAxiosError(error)) {
      if (error.response) {
        bamlOutputChannel.appendLine(
          `Axios Error: Status ${error.response.status} - ${error.response.statusText}`,
        );
      } else if (error.request) {
        bamlOutputChannel.appendLine(
          'Axios Error: No response received from server.',
        );
      } else {
        bamlOutputChannel.appendLine(
          `Axios Error: Request setup failed - ${error.message}`,
        );
      }
    }

    // Record failure for backoff
    backoffManager.recordFailure(cliVersion.version, bamlOutputChannel);

    return null;
  }
}

/**
 * Resolves the absolute path to the BAML CLI executable.
 */
export async function resolveCliPath(
  context: ExtensionContext,
  requestedVersion: string,
  bamlOutputChannel: OutputChannel,
): Promise<string | null> {
  bamlOutputChannel.appendLine(
    `Resolving CLI path for version: ${requestedVersion}`,
  );

  // Check if baml.cliPath is configured in user settings
  const { BAML_CONFIG_SINGLETON, refreshBamlConfigSingleton } = await import('../bamlConfig');
  refreshBamlConfigSingleton();
  
  const configuredCliPath = BAML_CONFIG_SINGLETON.config?.cliPath;
  if (configuredCliPath) {
    bamlOutputChannel.appendLine(
      `Using configured CLI path from baml.cliPath: ${configuredCliPath}`,
    );
    return configuredCliPath;
  }

  const packageJson = await import('../../../../package.json');
  const bundledVersion = packageJson.version as string;

  // Check if requested version matches bundled version
  if (
    semver.valid(requestedVersion) &&
    semver.valid(bundledVersion) &&
    semver.eq(requestedVersion, bundledVersion)
  ) {
    console.log(
      `Requested version (${requestedVersion}) matches bundled version (${bundledVersion}).`,
    );
    bamlOutputChannel.appendLine(
      `Requested version (${requestedVersion}) matches bundled version (${bundledVersion}).`,
    );

    // Use CLI from node_modules/@baml/cli/dist if present
    const bundledCliActualPath = getBundledCliPath(context);
    if (bundledCliActualPath) {
      await cacheBundledCli(
        context,
        bundledCliActualPath,
        bundledVersion,
        bamlOutputChannel,
      );
      bamlOutputChannel.appendLine(
        `Using CLI from node_modules: ${bundledCliActualPath}`,
      );
      return bundledCliActualPath;
    }

    bamlOutputChannel.appendLine(
      'WARN: Bundled version matches, but executable not found/accessible. Will attempt to use/download specific version.',
    );
  } else {
    bamlOutputChannel.appendLine(
      `Requested version (${requestedVersion}) does not match bundled version (${bundledVersion}). Will download specific version.`,
    );
  }

  // Check if already downloaded
  const cliVersionMeta: CliVersion = {
    architecture: os.arch(),
    platform: os.platform(),
    version: requestedVersion,
  };

  const expectedDownloadedPath = downloadedCliPath(context, cliVersionMeta);

  if (await checkIfDownloadedCliExists(context, cliVersionMeta)) {
    bamlOutputChannel.appendLine(
      `Found existing downloaded CLI for version ${requestedVersion}: ${expectedDownloadedPath}`,
    );

    if (ensureExecutablePermissions(expectedDownloadedPath)) {
      return expectedDownloadedPath;
    }

    bamlOutputChannel.appendLine(
      `ERROR: Downloaded CLI exists but permissions failed for ${expectedDownloadedPath}. Attempting re-download.`,
    );
  }

  // Check if we should attempt download based on backoff
  if (
    !backoffManager.shouldAttemptDownload(requestedVersion, bamlOutputChannel)
  ) {
    console.log(`Download blocked by backoff for version ${requestedVersion}. Falling back to bundled CLI.`);
    bamlOutputChannel.appendLine(
      `Download blocked by backoff for version ${requestedVersion}. Falling back to bundled CLI.`,
    );
    
    const bundledCliActualPath = getBundledCliPath(context);
    if (bundledCliActualPath) {
      bamlOutputChannel.appendLine(
        `Using bundled CLI as fallback: ${bundledCliActualPath}`,
      );
      return bundledCliActualPath;
    }
    
    return null;
  }

  // Attempt to download
  let downloadedPath: string | null = null;

  try {
    backoffManager.markDownloadStarted(requestedVersion);
    bamlOutputChannel.appendLine(
      `Attempting to download CLI version ${requestedVersion}... [Lock acquired]`,
    );

    // Show progress during download
    downloadedPath = await window.withProgress(
      {
        location: ProgressLocation.Notification,
        cancellable: false,
        title: `Downloading BAML Language Server v${requestedVersion}`,
      },
      async () => {
        return await downloadCli(context, cliVersionMeta, bamlOutputChannel);
      },
    );
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    bamlOutputChannel.appendLine(
      `ERROR: Unexpected error during download process for ${requestedVersion}: ${errorMsg}`,
    );
    downloadedPath = null;
  } finally {
    backoffManager.markDownloadCompleted(requestedVersion);
    bamlOutputChannel.appendLine(
      `Download attempt for ${requestedVersion} finished. [Lock released]`,
    );
  }

  if (downloadedPath) {
    window.showInformationMessage(
      `BAML CLI v${requestedVersion} downloaded successfully!`,
    );
    return downloadedPath;
  }

  // Download failed - fallback to bundled CLI
  console.log(`Download failed for version ${requestedVersion}. Falling back to bundled CLI.`);
  bamlOutputChannel.appendLine(
    `ERROR: Failed to resolve CLI path for version ${requestedVersion} after download attempt. Falling back to bundled CLI.`,
  );
  
  const bundledCliActualPath = getBundledCliPath(context);
  if (bundledCliActualPath) {
    bamlOutputChannel.appendLine(
      `Using bundled CLI as fallback: ${bundledCliActualPath}`,
    );
    return bundledCliActualPath;
  }

  bamlOutputChannel.appendLine(
    `ERROR: No bundled CLI available as fallback for version ${requestedVersion}.`,
  );
  return null;
}
