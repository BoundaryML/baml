import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { type ExtensionContext, window } from 'vscode';
import { getTargetTriple, getExecutableName } from './platform';
import { ensureExecutablePermissions } from './paths';

/**
 * Finds the path to the bundled CLI executable
 */
export function getBundledCliPath(context: ExtensionContext): string | null {
  const executableName = getExecutableName();
  const targetTriple = getTargetTriple();

  console.log(`Trying to find bundled CLI for triple: ${targetTriple}`);

  // 1. Check node_modules/.bin first (workspace development)
  const nodeModulesPath = checkNodeModulesBin(context, executableName);
  if (nodeModulesPath) return nodeModulesPath;

  // 2. Check Development Override Path (legacy dist/baml-cli)
  const devPath = checkDevOverridePath(context, executableName);
  if (devPath) return devPath;

  // 3. Check Standard Bundled Path
  if (targetTriple) {
    const standardPath = checkStandardBundledPath(
      context,
      targetTriple,
      executableName,
    );
    if (standardPath) return standardPath;

    // 4. Linux MUSL Fallback
    const muslPath = checkLinuxMuslFallback(
      context,
      targetTriple,
      executableName,
    );
    if (muslPath) return muslPath;
  } else {
    console.warn(
      'Target triple could not be determined, cannot check standard bundled path.',
    );
  }

  console.log('Bundled CLI executable not found in expected locations.');
  return null;
}

/**
 * Checks for CLI in node_modules/.bin
 */
function checkNodeModulesBin(
  context: ExtensionContext,
  executableName: string,
): string | null {
  const nodeModulesBinPath = context.asAbsolutePath(
    path.join('node_modules', '@baml', 'cli', 'dist', 'baml-cli'),
  );

  console.log('nodeModulesBinPath', nodeModulesBinPath);

  if (fs.existsSync(nodeModulesBinPath)) {
    console.log('Found CLI in node_modules/@baml/cli/dist:', nodeModulesBinPath);
    if (ensureExecutablePermissions(nodeModulesBinPath)) {
      return nodeModulesBinPath;
    }
    console.error(
      `CLI found in node_modules/@baml/cli/dist but failed to set permissions: ${nodeModulesBinPath}`,
    );
    showPermissionError(nodeModulesBinPath);
  }

  return null;
}

/**
 * Checks for CLI in development override path
 */
function checkDevOverridePath(
  context: ExtensionContext,
  executableName: string,
): string | null {
  const devServerPath = context.asAbsolutePath(
    path.join('dist', executableName),
  );

  console.log('devServerPath', devServerPath);

  if (fs.existsSync(devServerPath)) {
    console.log('Found bundled CLI at development override path:', devServerPath);
    if (ensureExecutablePermissions(devServerPath)) {
      return devServerPath;
    }
    console.error(
      `Development override CLI found but failed to set permissions: ${devServerPath}`,
    );
    showPermissionError(devServerPath);
  }

  return null;
}

/**
 * Checks for CLI in standard bundled path
 */
function checkStandardBundledPath(
  context: ExtensionContext,
  targetTriple: string,
  executableName: string,
): string | null {
  const primaryBundledPath = context.asAbsolutePath(
    path.join('dist', targetTriple, executableName),
  );

  console.log(`Checking standard bundled path: ${primaryBundledPath}`);

  if (fs.existsSync(primaryBundledPath)) {
    console.log('Found bundled CLI at standard path:', primaryBundledPath);
    if (ensureExecutablePermissions(primaryBundledPath)) {
      return primaryBundledPath;
    }
    console.error(
      `Standard bundled CLI found but failed to set permissions: ${primaryBundledPath}`,
    );
    showPermissionError(primaryBundledPath);
  }

  return null;
}

/**
 * Checks for Linux MUSL fallback
 */
function checkLinuxMuslFallback(
  context: ExtensionContext,
  targetTriple: string,
  executableName: string,
): string | null {
  if (os.platform() === 'linux' && targetTriple.endsWith('-gnu')) {
    const muslTargetTriple = targetTriple.replace('-gnu', '-musl');
    const muslBundledPath = context.asAbsolutePath(
      path.join('dist', muslTargetTriple, executableName),
    );

    console.log(`Checking Linux MUSL fallback path: ${muslBundledPath}`);

    if (fs.existsSync(muslBundledPath)) {
      console.log('Found bundled CLI at MUSL fallback path:', muslBundledPath);
      if (ensureExecutablePermissions(muslBundledPath)) {
        return muslBundledPath;
      }
      console.error(
        `MUSL fallback CLI found but failed to set permissions: ${muslBundledPath}`,
      );
      showPermissionError(muslBundledPath);
    }
  }

  return null;
}

/**
 * Shows error message for permission failures
 */
function showPermissionError(filePath: string): void {
  window.showErrorMessage(
    `Failed to set permissions for BAML Language Server executable. Please check file permissions at ${filePath}`,
  );
}