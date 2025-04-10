/**
 * Language server is bundled in the baml CLI, so we need to download the
 * correct binary based on the version.
 */

import os from 'os'
import path from 'path'
import fs from 'fs/promises'
import { pipeline } from 'stream/promises'
import { createWriteStream } from 'fs'
import axios from 'axios'
import * as tar from 'tar'
import AdmZip from 'adm-zip'

type CliVersion = {
  architecture: string
  platform: string
  version: string
}

const BASE_URL = 'https://github.com/BoundaryML/baml/releases/download'

// TODO: $HOME/.baml for Linux, should use XDG? KnownFolders on Windows?
const INSTALL_PATH = path.join(os.homedir(), '.baml')

/**
 * Returns the architecture name correctly formatted for the Github release.
 *
 * @param nodeArch The architecture of the Node.js runtime.
 * @returns The architecture for the release.
 */
function getReleaseArchitecture(nodeArch: string): string {
  switch (nodeArch) {
    case 'arm64':
      return 'aarch64'
    case 'x64':
      return 'x86_64'
    default:
      return nodeArch
  }
}

/**
 * Returns the platform name correctly formatted for the Github release.
 *
 * @param platform Current Node.js platform.
 * @returns The platform for the release.
 */
function getReleasePlatform(platform: string): string {
  switch (platform) {
    case 'win32':
      return 'pc-windows-msvc'
    case 'darwin':
      return 'apple-darwin'
    // TODO: linux-musl
    case 'linux':
      return 'unknown-linux-gnu'
    default:
      return platform
  }
}

/**
 * Returns the extension for the compressed file for the Github release.
 *
 * @param platform Current Node.js platform (already formatted).
 * @returns The extension for the compressed file.
 */
function getCliCompressedFileExtension(platform: string): string {
  switch (getReleasePlatform(platform)) {
    case 'pc-windows-msvc':
      return 'zip'
    case 'apple-darwin':
    case 'unknown-linux-gnu':
    case 'unknown-linux-musl':
      return 'tar.gz'
    default:
      return 'zip' // TODO: Throw error or something.
  }
}

/**
 * Returns the filename of the CLI binary for the given platform, architecture
 * and version.
 *
 * @param platform Current Node.js platform.
 * @param architecture The architecture of the Node.js runtime.
 * @param version The version of the CLI.
 *
 * @returns The filename of the CLI binary.
 */
function cliBinaryFileName({ architecture, platform, version }: CliVersion): string {
  architecture = getReleaseArchitecture(architecture)
  platform = getReleasePlatform(platform)

  return `baml-cli-${version}-${architecture}-${platform}`
}

/**
 * Returns the full path to the CLI binary for the given platform, architecture
 * and version.
 *
 * @param cliVersion The version of the CLI.
 *
 * @returns The full path to the CLI binary.
 */
export function cliBinaryPath(cliVersion: CliVersion): string {
  return path.join(INSTALL_PATH, cliBinaryFileName(cliVersion))
}

/**
 * Checks if the CLI binary exists in the installation path.
 *
 * @param binaryFileName The filename of the CLI binary.
 *
 * @returns True if the CLI binary exists, false otherwise.
 */
export async function checkIfCliBinaryExists(cliVersion: CliVersion): Promise<boolean> {
  return await fs
    .access(cliBinaryPath(cliVersion))
    .then(() => true)
    .catch(() => false)
}

/**
 * Entry point to download the CLI binary.
 *
 * @param cliVersion CLI metadata, platform-architecture-version.
 */
export async function downloadCli(cliVersion: CliVersion): Promise<void> {
  // TODO: Testing
  cliVersion.version = '0.1.0'
  // cliVersion.platform = 'win32'

  // Filenames.
  const binaryFileName = cliBinaryFileName(cliVersion)
  const extension = getCliCompressedFileExtension(cliVersion.platform)

  // Complete filename in the Github release.
  const compressedFileName = `${binaryFileName}.${extension}`

  // Github release download URL.
  const url = `${BASE_URL}/${cliVersion.version}/${compressedFileName}`

  console.log('LSP Download URL', url)

  // Make HTTP request, follow redirects.
  const res = await axios.get(url, { responseType: 'stream' })

  if (res.status !== 200) {
    throw new Error(`Failed to download CLI: HTTP ${res.status}`)
  }

  // Create binaries directory if it doesn't exist.
  try {
    await fs.access(INSTALL_PATH)
  } catch (e) {
    await fs.mkdir(INSTALL_PATH, { recursive: true })
  }

  // Extract the compressed file to the installation path.
  await extractFile(res.data, extension, binaryFileName, compressedFileName)
}

/**
 * Extracts the compressed file to the installation path.
 *
 * @param source The source stream of the compressed file.
 * @param extension The extension of the compressed file.
 * @param binaryFileName The filename of the CLI binary.
 * @param compressedFileName The filename of the compressed file.
 *
 * @returns A promise that resolves when the file is fully extracted and written
 * to disk.
 */
async function extractFile(
  source: ReadableStream,
  extension: string,
  binaryFileName: string,
  compressedFileName: string,
): Promise<void> {
  if (extension === 'tar.gz') {
    await pipeline(
      source,
      tar.extract({ cwd: INSTALL_PATH, onReadEntry: (entry) => (entry.path = binaryFileName) }, ['./baml-cli']),
    )
  } else if (extension === 'zip') {
    const compressedFilePath = path.join(INSTALL_PATH, compressedFileName)
    await pipeline(source, createWriteStream(compressedFilePath))

    // Due to the zip file format, we can't use streaming APIs, we need the
    // entire content to be downloaded before we can extract the binary.
    const zip = new AdmZip(compressedFilePath)
    zip.extractEntryTo('./baml-cli.exe', INSTALL_PATH, false, true, false, binaryFileName)

    // TODO: Don't know why keeping the original permissions in the call above
    // doesn't work.
    const binaryFilePath = path.join(INSTALL_PATH, binaryFileName)
    await fs.chmod(binaryFilePath, 0o755)

    // Remove the compressed file.
    await fs.unlink(compressedFilePath)
  } else {
    throw new Error(`Unsupported compressed file format for LSP download: ${extension}`)
  }
}
