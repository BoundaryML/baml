import { execSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';

/**
 * Returns the architecture name correctly formatted for the Github release.
 */
export function getReleaseArchitecture(nodeArch: string): string {
  switch (nodeArch) {
    case 'arm64':
      return 'aarch64';
    case 'x64':
      return 'x86_64';
    default:
      console.warn(`Unknown architecture: ${nodeArch}. Using as is.`);
      return nodeArch;
  }
}

/**
 * Returns the platform name correctly formatted for the Github release.
 */
export function getReleasePlatform(platform: string): string {
  switch (platform) {
    case 'win32':
      return 'pc-windows-msvc';
    case 'darwin':
      return 'apple-darwin';
    case 'linux':
      return detectLinuxLibc();
    default:
      console.warn(`Unknown platform: ${platform}. Using as is.`);
      return platform;
  }
}

/**
 * Detects whether the Linux system uses musl or gnu libc
 */
function detectLinuxLibc(): string {
  try {
    // Check if we're on Alpine Linux, which uses musl
    const isAlpine = fs.existsSync('/etc/alpine-release');
    if (isAlpine) {
      return 'unknown-linux-musl';
    }

    // Try to check libc by running ldd --version
    const lddOutput = execSync('ldd --version 2>&1 || true')
      .toString()
      .toLowerCase();

    if (lddOutput.includes('musl')) {
      return 'unknown-linux-musl';
    }
  } catch (error) {
    console.warn('Failed to detect libc type:', error);
  }

  // Default to gnu if detection fails
  return 'unknown-linux-gnu';
}

/**
 * Determines the target triple string based on platform and architecture.
 */
export function getTargetTriple(): string | null {
  const platform = os.platform();
  const arch = os.arch();
  const releaseArch = getReleaseArchitecture(arch);
  const releasePlatform = getReleasePlatform(platform);

  return `${releaseArch}-${releasePlatform}`;
}

/**
 * Returns the extension for the compressed file for the Github release.
 */
export function getCliCompressedFileExtension(platform: string): string {
  const releasePlatform = getReleasePlatform(platform);
  
  switch (releasePlatform) {
    case 'pc-windows-msvc':
      return 'zip';
    case 'apple-darwin':
    case 'unknown-linux-gnu':
    case 'unknown-linux-musl':
      return 'tar.gz';
    default:
      console.warn(
        `Unsupported platform ${releasePlatform} for compression extension, defaulting to zip.`,
      );
      return 'zip';
  }
}

/**
 * Returns the expected name of the executable file.
 */
export function getExecutableName(): string {
  return os.platform() === 'win32' ? 'baml-cli.exe' : 'baml-cli';
}