import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { pipeline } from 'node:stream/promises';
import AdmZip from 'adm-zip';
import * as tar from 'tar';
import type { OutputChannel } from 'vscode';
import { getExecutableName } from './platform';

/**
 * Extracts the compressed file to the installation path.
 */
export async function extractFile(
  sourcePath: string,
  extension: string,
  targetFileName: string,
  installPath: string,
  outputChannel: OutputChannel,
): Promise<void> {
  const binaryFilePath = path.join(installPath, targetFileName);
  outputChannel.appendLine(`Extracting ${sourcePath} to: ${binaryFilePath}`);

  if (extension === 'tar.gz') {
    await extractTarGz(sourcePath, targetFileName, installPath, outputChannel);
  } else if (extension === 'zip') {
    await extractZip(sourcePath, targetFileName, installPath, outputChannel);
  } else {
    throw new Error(
      `Unsupported compressed file format for LSP download: ${extension}`,
    );
  }

  await setExecutablePermissions(binaryFilePath, outputChannel);
}

/**
 * Extracts a tar.gz file
 */
async function extractTarGz(
  sourcePath: string,
  targetFileName: string,
  installPath: string,
  outputChannel: OutputChannel,
): Promise<void> {
  const sourceStream = fs.createReadStream(sourcePath);
  await pipeline(
    sourceStream,
    tar.extract(
      {
        cwd: installPath,
        onentry: (entry) => {
          entry.path = targetFileName;
        },
      },
      ['./baml-cli'],
    ),
  );
  outputChannel.appendLine(
    `Tarball extracted and renamed to: ${path.join(installPath, targetFileName)}`,
  );
}

/**
 * Extracts a zip file
 */
async function extractZip(
  sourcePath: string,
  targetFileName: string,
  installPath: string,
  outputChannel: OutputChannel,
): Promise<void> {
  outputChannel.appendLine(`Extracting from verified zip: ${sourcePath}`);

  const zip = new AdmZip(sourcePath);
  const zipEntryName = getExecutableName();
  const entry = zip.getEntry(zipEntryName);

  if (!entry) {
    outputChannel.appendLine(
      `ERROR: Could not find entry '${zipEntryName}' in the verified zip file: ${sourcePath}`,
    );
    throw new Error(
      `Required entry '${zipEntryName}' not found in zip archive.`,
    );
  }

  outputChannel.appendLine(
    `Found entry ${zipEntryName} in zip. Extracting to ${path.join(installPath, targetFileName)}`,
  );

  zip.extractEntryTo(entry, installPath, false, true, false, targetFileName);

  outputChannel.appendLine(
    `Extracted ${zipEntryName} to ${path.join(installPath, targetFileName)}`,
  );
}

/**
 * Sets executable permissions on Unix systems
 */
async function setExecutablePermissions(
  binaryFilePath: string,
  outputChannel: OutputChannel,
): Promise<void> {
  if (os.platform() !== 'win32') {
    try {
      outputChannel.appendLine(
        `Setting executable permissions for: ${binaryFilePath}`,
      );
      await fs.promises.chmod(binaryFilePath, 0o755);
    } catch (err: any) {
      outputChannel.appendLine(
        `ERROR: Failed to set executable permissions for ${binaryFilePath}: ${err}`,
      );
      throw new Error(
        `Failed to set executable permissions for ${binaryFilePath}`,
      );
    }
  }
}
