import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { ExtensionContext } from 'vscode';
import {
  getExecutableName,
  getReleaseArchitecture,
  getReleasePlatform,
} from './platform';
import type { CliVersion } from './types';

/**
 * Gets the base installation path for BAML CLI
 */
export function getInstallPath(context: ExtensionContext): string {
  // Prefer globalStorageUri if available, otherwise fallback to ~/.baml
  // For simplicity here, we'll use ~/.baml
  return path.join(os.homedir(), '.baml');
}

/**
 * Returns the filename of the CLI binary artifact (without compression extension).
 */
export function cliBinaryArtifactName({
  architecture,
  platform,
  version,
}: CliVersion): string {
  const releaseArchitecture = getReleaseArchitecture(architecture);
  const releasePlatform = getReleasePlatform(platform);
  return `baml-cli-${version}-${releaseArchitecture}-${releasePlatform}`;
}

/**
 * Returns the full path to the potentially downloaded CLI binary.
 */
export function downloadedCliPath(
  context: ExtensionContext,
  cliVersion: CliVersion,
): string {
  const installPath = getInstallPath(context);
  const uniqueFileName = `${cliBinaryArtifactName(cliVersion)}-${getExecutableName()}`;
  return path.join(installPath, uniqueFileName);
}

/**
 * Checks if the downloaded CLI binary exists.
 */
export async function checkIfDownloadedCliExists(
  context: ExtensionContext,
  cliVersion: CliVersion,
): Promise<boolean> {
  const expectedPath = downloadedCliPath(context, cliVersion);
  console.log(`Checking existence of downloaded CLI at: ${expectedPath}`);
  return fs.promises
    .access(expectedPath)
    .then(() => true)
    .catch(() => false);
}

/**
 * Ensures the directory for storing downloaded CLIs exists.
 */
export async function ensureInstallPathExists(
  context: ExtensionContext,
): Promise<string> {
  const installPath = getInstallPath(context);
  try {
    await fs.promises.access(installPath);
  } catch (e) {
    console.log(`Creating BAML CLI install directory: ${installPath}`);
    await fs.promises.mkdir(installPath, { recursive: true });
  }
  return installPath;
}

/**
 * Helper to set executable permissions (non-Windows).
 */
export function ensureExecutablePermissions(filePath: string): boolean {
  if (os.platform() !== 'win32') {
    try {
      fs.chmodSync(filePath, 0o755);
      console.log(`Ensured executable permissions for: ${filePath}`);
      return true;
    } catch (err: any) {
      console.error(`Failed to chmod server executable at ${filePath}: ${err}`);
      return false;
    }
  }
  return true; // No-op on Windows, always succeeds
}
