import crypto from 'node:crypto';
import { createWriteStream } from 'node:fs';
import fs from 'node:fs';
import path from 'node:path';
import { pipeline } from 'node:stream/promises';
import axios from 'axios';
import type { OutputChannel } from 'vscode';
import { BASE_URL, DOWNLOAD_TIMEOUT } from './constants';
import { extractFile } from './extraction';
import { cliBinaryArtifactName } from './paths';
import { getCliCompressedFileExtension, getExecutableName } from './platform';
import type { CliVersion } from './types';

/**
 * Downloads and verifies a CLI binary
 */
export async function downloadAndVerifyCli(
  cliVersion: CliVersion,
  installPath: string,
  outputChannel: OutputChannel,
): Promise<{ targetFullPath: string; extension: string } | null> {
  const artifactName = cliBinaryArtifactName(cliVersion);
  const extension = getCliCompressedFileExtension(cliVersion.platform);
  const compressedFileName = `${artifactName}.${extension}`;
  const compressedFileTempPath = path.join(
    installPath,
    `${compressedFileName}.tmp`,
  );
  const targetFileName = `${artifactName}-${getExecutableName()}`;
  const targetFullPath = path.join(installPath, targetFileName);

  const binaryUrl = `${BASE_URL}/${cliVersion.version}/${compressedFileName}`;
  const checksumUrl = `${BASE_URL}/${cliVersion.version}/${compressedFileName}.sha256`;

  outputChannel.appendLine(
    `Attempting download for BAML CLI v${cliVersion.version}`,
  );
  outputChannel.appendLine(` Binary URL: ${binaryUrl}`);
  outputChannel.appendLine(` Checksum URL: ${checksumUrl}`);
  outputChannel.appendLine(` Target Path: ${targetFullPath}`);

  let downloadSucceeded = false;

  try {
    // Download binary
    downloadSucceeded = await downloadBinary(
      binaryUrl,
      compressedFileTempPath,
      outputChannel,
    );

    // Download and verify checksum
    const expectedChecksum = await downloadChecksum(checksumUrl, outputChannel);
    await verifyChecksum(
      compressedFileTempPath,
      expectedChecksum,
      outputChannel,
    );

    // Extract the verified file
    outputChannel.appendLine('Proceeding with extraction...');
    await extractFile(
      compressedFileTempPath,
      extension,
      targetFileName,
      installPath,
      outputChannel,
    );

    outputChannel.appendLine(
      `Successfully downloaded, verified, and extracted BAML CLI to: ${targetFullPath}`,
    );

    return { targetFullPath, extension };
  } finally {
    // Clean up temporary file
    if (downloadSucceeded) {
      await cleanupTempFile(compressedFileTempPath, outputChannel);
    }
  }
}

/**
 * Downloads the binary file
 */
async function downloadBinary(
  binaryUrl: string,
  targetPath: string,
  outputChannel: OutputChannel,
): Promise<boolean> {
  outputChannel.appendLine(`Downloading binary to ${targetPath}...`);

  const res = await axios.get(binaryUrl, {
    responseType: 'stream',
    timeout: DOWNLOAD_TIMEOUT.BINARY,
    validateStatus: (status) => status >= 200 && status < 300,
  });

  await pipeline(res.data, createWriteStream(targetPath));
  outputChannel.appendLine(`Binary download complete: ${targetPath}`);

  return true;
}

/**
 * Downloads and parses the checksum file
 */
async function downloadChecksum(
  checksumUrl: string,
  outputChannel: OutputChannel,
): Promise<string> {
  outputChannel.appendLine(`Downloading checksum from ${checksumUrl}...`);

  try {
    const checksumRes = await axios.get(checksumUrl, {
      responseType: 'text',
      timeout: DOWNLOAD_TIMEOUT.CHECKSUM,
      validateStatus: (status) => status === 200,
    });

    const checksumContent = checksumRes.data.trim();
    const expectedChecksum = checksumContent.split(/\s+/)[0];

    if (!/^[a-f0-9]{64}$/i.test(expectedChecksum)) {
      throw new Error(`Invalid checksum format received: ${expectedChecksum}`);
    }

    outputChannel.appendLine(`Expected checksum: ${expectedChecksum}`);
    return expectedChecksum;
  } catch (error: any) {
    outputChannel.appendLine(
      `ERROR: Failed to download or parse checksum file: ${error.message}`,
    );

    if (axios.isAxiosError(error) && error.response?.status === 404) {
      throw new Error(
        `Checksum file not found at ${checksumUrl}. Aborting download for security.`,
      );
    }

    throw error;
  }
}

/**
 * Verifies the SHA256 checksum of the downloaded file
 */
async function verifyChecksum(
  filePath: string,
  expectedChecksum: string,
  outputChannel: OutputChannel,
): Promise<void> {
  outputChannel.appendLine(`Calculating SHA256 hash for ${filePath}...`);

  const fileBuffer = await fs.promises.readFile(filePath);
  const calculatedHash = crypto
    .createHash('sha256')
    .update(fileBuffer)
    .digest('hex');

  outputChannel.appendLine(`Calculated checksum: ${calculatedHash}`);

  if (calculatedHash !== expectedChecksum) {
    throw new Error(
      `Checksum mismatch! Expected ${expectedChecksum}, got ${calculatedHash}. File may be corrupted or tampered with.`,
    );
  }

  outputChannel.appendLine('Checksum verification successful!');
}

/**
 * Cleans up temporary downloaded files
 */
async function cleanupTempFile(
  tempPath: string,
  outputChannel: OutputChannel,
): Promise<void> {
  try {
    outputChannel.appendLine(`Cleaning up temporary file: ${tempPath}`);
    await fs.promises.unlink(tempPath);
    outputChannel.appendLine('Temporary file deleted.');
  } catch (error: any) {
    outputChannel.appendLine(
      `WARN: Failed to delete temporary download file ${tempPath}: ${error.message}`,
    );
  }
}
