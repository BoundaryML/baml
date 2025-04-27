import os from 'os'
import path from 'path'
import fs from 'fs' // Use synchronous fs methods where appropriate for path checking
import { pipeline } from 'stream/promises'
import { createWriteStream } from 'fs'
import axios from 'axios'
import * as tar from 'tar'
import AdmZip from 'adm-zip'
import * as crypto from 'crypto' // Added for hash calculation
import { ExtensionContext, OutputChannel, ProgressLocation, window } from 'vscode' // Import ExtensionContext, window, and OutputChannel
import semver from 'semver' // Import semver

// Assuming packageJson is required similarly to index.ts to get the bundled version
const packageJson = require('../../../../package.json') // Adjust path as needed

// Backoff constants
const INITIAL_BACKOFF_DELAY_MS = 10 * 60 * 1000 // 10 minutes
const MAX_BACKOFF_DELAY_MS = 60 * 60 * 1000 // 1 hour
const MAX_FAILURE_COUNT_BEFORE_RESET = 5 // Reset backoff after this many failures to allow retries later

// Backoff state (maps version string to failure info)
const downloadBackoffState = new Map<string, { failureCount: number; lastAttemptTimestamp: number }>()
// Set to track versions currently being downloaded
const downloadsInProgress = new Set<string>()

export type CliVersion = {
  architecture: string
  platform: string
  version: string
}

const BASE_URL = 'https://github.com/BoundaryML/baml/releases/download'
// Use ExtensionContext's globalStorageUri for a more robust storage path
const getInstallPath = (context: ExtensionContext): string => {
  // Prefer globalStorageUri if available, otherwise fallback to ~/.baml
  // Note: globalStorageUri might need async creation on first access.
  // For simplicity here, we'll assume it exists or handle creation elsewhere.
  // Let's stick to ~/.baml for now to avoid async complexities here.
  return path.join(os.homedir(), '.baml')
}

/**
 * Returns the architecture name correctly formatted for the Github release.
 * Public for reuse.
 * @param nodeArch The architecture of the Node.js runtime.
 * @returns The architecture for the release.
 */
export function getReleaseArchitecture(nodeArch: string): string {
  switch (nodeArch) {
    case 'arm64':
      return 'aarch64'
    case 'x64':
      return 'x86_64'
    default:
      // Should we throw or return unknown? Let's return nodeArch for now.
      console.warn(`Unknown architecture: ${nodeArch}. Using as is.`)
      return nodeArch
  }
}

/**
 * Returns the platform name correctly formatted for the Github release.
 * Public for reuse.
 * @param platform Current Node.js platform.
 * @returns The platform for the release.
 */
export function getReleasePlatform(platform: string): string {
  switch (platform) {
    case 'win32':
      return 'pc-windows-msvc'
    case 'darwin':
      return 'apple-darwin'
    case 'linux':
      // Attempt to detect musl vs gnu
      // This is a best-effort detection as it's difficult to reliably detect in a VSCode extension
      try {
        // Check if we're on Alpine Linux, which uses musl
        const isAlpine = fs.existsSync('/etc/alpine-release')
        if (isAlpine) {
          return 'unknown-linux-musl'
        }

        // Try to check libc by running ldd --version
        const { execSync } = require('child_process')
        const lddOutput = execSync('ldd --version 2>&1 || true').toString().toLowerCase()

        if (lddOutput.includes('musl')) {
          return 'unknown-linux-musl'
        }
      } catch (error) {
        console.warn('Failed to detect libc type:', error)
        // Fall through to default
      }

      // Default to gnu if detection fails
      return 'unknown-linux-gnu'
    default:
      console.warn(`Unknown platform: ${platform}. Using as is.`)
      return platform
  }
}

/**
 * Determines the target triple string based on platform and architecture.
 * Internal helper.
 */
function _getTargetTriple(): string | null {
  const platform = os.platform()
  const arch = os.arch()
  const releaseArch = getReleaseArchitecture(arch)
  const releasePlatform = getReleasePlatform(platform)

  return `${releaseArch}-${releasePlatform}`
}

/**
 * Returns the extension for the compressed file for the Github release.
 * Internal helper.
 * @param platform Current Node.js platform.
 * @returns The extension for the compressed file.
 */
function _getCliCompressedFileExtension(platform: string): string {
  // Use the *release* platform name
  switch (getReleasePlatform(platform)) {
    case 'pc-windows-msvc':
      return 'zip'
    case 'apple-darwin':
    case 'unknown-linux-gnu':
    case 'unknown-linux-musl': // Explicitly handle potential musl
      return 'tar.gz'
    default:
      console.warn(`Unsupported platform ${getReleasePlatform(platform)} for compression extension, defaulting to zip.`)
      return 'zip' // Fallback, might fail
  }
}

/**
 * Returns the filename of the CLI binary artifact (without compression extension).
 * Internal helper.
 * @param cliVersion The version metadata of the CLI.
 * @returns The filename of the CLI artifact.
 */
function cliBinaryArtifactName({ architecture, platform, version }: CliVersion): string {
  const releaseArchitecture = getReleaseArchitecture(architecture)
  const releasePlatform = getReleasePlatform(platform)
  // Construct the name format used in releases
  return `baml-cli-${version}-${releaseArchitecture}-${releasePlatform}`
}

/**
 * Returns the expected name of the executable file itself.
 * Internal helper.
 */
function _getExecutableName(): string {
  return os.platform() === 'win32' ? 'baml-cli.exe' : 'baml-cli'
}

/**
 * Returns the full path to the potentially *downloaded* CLI binary.
 * Public for explicit checks if needed.
 * @param context Extension context to determine storage path.
 * @param cliVersion The version of the CLI.
 * @returns The full path to where the CLI binary *should* be if downloaded.
 */
export function downloadedCliPath(context: ExtensionContext, cliVersion: CliVersion): string {
  const installPath = getInstallPath(context)
  // The filename inside the archive might just be 'baml-cli' or 'baml-cli.exe'
  // but we might store it uniquely per version/arch/platform in our folder.
  // Let's store it using the artifact name for clarity.
  // However, the extraction logic currently renames it. Let's align this.
  // Let's store it with a versioned name for clarity.
  const uniqueFileName = `${cliBinaryArtifactName(cliVersion)}-${_getExecutableName()}`
  return path.join(installPath, uniqueFileName)
}

/**
 * Checks if the downloaded CLI binary exists.
 * Public for explicit checks.
 * @param context Extension context.
 * @param cliVersion The version of the CLI.
 * @returns True if the CLI binary exists, false otherwise.
 */
export async function checkIfDownloadedCliExists(context: ExtensionContext, cliVersion: CliVersion): Promise<boolean> {
  const expectedPath = downloadedCliPath(context, cliVersion)
  console.log(`Checking existence of downloaded CLI at: ${expectedPath}`)
  return fs.promises
    .access(expectedPath)
    .then(() => true)
    .catch(() => false)
}

/**
 * Ensures the directory for storing downloaded CLIs exists.
 * Internal helper.
 */
async function _ensureInstallPathExists(context: ExtensionContext): Promise<string> {
  const installPath = getInstallPath(context)
  try {
    await fs.promises.access(installPath)
  } catch (e) {
    console.log(`Creating BAML CLI install directory: ${installPath}`)
    await fs.promises.mkdir(installPath, { recursive: true })
  }
  return installPath
}

/**
 * Extracts the compressed file to the installation path.
 * Internal helper.
 */
async function _extractFile(
  sourcePath: string, // Changed from stream to path
  extension: string,
  targetFileName: string, // The final name we want for the executable
  installPath: string,
  bamlOutputChannel: OutputChannel, // Added output channel
): Promise<void> {
  const binaryFilePath = path.join(installPath, targetFileName)
  bamlOutputChannel.appendLine(`Extracting ${sourcePath} to: ${binaryFilePath}`)

  if (extension === 'tar.gz') {
    // Extract directly from the verified source file, renaming the entry on the fly
    const sourceStream = fs.createReadStream(sourcePath)
    await pipeline(
      sourceStream,
      tar.extract({ cwd: installPath, onReadEntry: (entry) => (entry.path = targetFileName) }, ['./baml-cli']),
    )
    bamlOutputChannel.appendLine(`Tarball extracted and renamed to: ${binaryFilePath}`)
  } else if (extension === 'zip') {
    // The sourcePath is already the downloaded zip file
    bamlOutputChannel.appendLine(`Extracting from verified zip: ${sourcePath}`)

    // Extract the single executable file and rename it
    const zip = new AdmZip(sourcePath) // AdmZip can take a path
    // Find the entry corresponding to the executable name
    const zipEntryName = _getExecutableName() // e.g., baml-cli.exe or baml-cli
    const entry = zip.getEntry(zipEntryName)

    if (entry) {
      bamlOutputChannel.appendLine(`Found entry ${zipEntryName} in zip. Extracting to ${binaryFilePath}`)
      // Extract the entry directly to the final path with the final name
      zip.extractEntryTo(entry, installPath, false, true, false, targetFileName)
      bamlOutputChannel.appendLine(`Extracted ${zipEntryName} to ${binaryFilePath}`)
    } else {
      bamlOutputChannel.appendLine(
        `ERROR: Could not find entry '${zipEntryName}' in the verified zip file: ${sourcePath}`,
      )
      // No need to delete sourcePath here, caller (downloadCli) handles cleanup
      throw new Error(`Required entry '${zipEntryName}' not found in zip archive.`)
    }

    // Cleanup of the sourcePath (temp zip) is handled by the caller (downloadCli)
    bamlOutputChannel.appendLine(`Extraction from ${sourcePath} complete.`)
  } else {
    throw new Error(`Unsupported compressed file format for LSP download: ${extension}`)
  }

  // Ensure execute permissions after extraction (common step)
  if (os.platform() !== 'win32') {
    try {
      bamlOutputChannel.appendLine(`Setting executable permissions for: ${binaryFilePath}`)
      await fs.promises.chmod(binaryFilePath, 0o755)
    } catch (err: any) {
      bamlOutputChannel.appendLine(`ERROR: Failed to set executable permissions for ${binaryFilePath}: ${err}`)
      // Decide if this is fatal or just a warning
      throw new Error(`Failed to set executable permissions for ${binaryFilePath}`)
    }
  }
}

/**
 * Downloads and extracts the CLI binary for a specific version.
 * Public for explicit downloads if needed.
 * @param context Extension context.
 * @param cliVersion The specific version to download.
 * @returns The absolute path to the downloaded executable, or null if download fails.
 */
export async function downloadCli(
  context: ExtensionContext,
  cliVersion: CliVersion,
  bamlOutputChannel: OutputChannel,
): Promise<string | null> {
  const installPath = await _ensureInstallPathExists(context)
  const artifactName = cliBinaryArtifactName(cliVersion)
  const extension = _getCliCompressedFileExtension(cliVersion.platform)
  const compressedFileName = `${artifactName}.${extension}`
  const compressedFileTempPath = path.join(installPath, `${compressedFileName}.tmp`) // Temporary path for download
  const checksumFileName = `${compressedFileName}.sha256`
  const targetFileName = `${artifactName}-${_getExecutableName()}` // Unique name for storage
  const targetFullPath = path.join(installPath, targetFileName)

  const binaryUrl = `${BASE_URL}/${cliVersion.version}/${compressedFileName}`
  const checksumUrl = `${BASE_URL}/${cliVersion.version}/${checksumFileName}`

  bamlOutputChannel.appendLine(`Attempting download for BAML CLI v${cliVersion.version}`)
  bamlOutputChannel.appendLine(` Binary URL: ${binaryUrl}`)
  bamlOutputChannel.appendLine(` Checksum URL: ${checksumUrl}`)
  bamlOutputChannel.appendLine(` Target Path: ${targetFullPath}`)
  bamlOutputChannel.appendLine(` Temp Download Path: ${compressedFileTempPath}`)

  let downloadSucceeded = false
  let checksumVerified = false

  try {
    // 1. Download binary to temporary file
    bamlOutputChannel.appendLine(`Downloading binary to ${compressedFileTempPath}...`)
    const res = await axios.get(binaryUrl, {
      responseType: 'stream',
      timeout: 60000, // Increased timeout to 60 seconds
      validateStatus: (status) => status >= 200 && status < 300,
    })
    await pipeline(res.data, createWriteStream(compressedFileTempPath))
    bamlOutputChannel.appendLine(`Binary download complete: ${compressedFileTempPath}`)
    downloadSucceeded = true

    // 2. Download checksum file
    bamlOutputChannel.appendLine(`Downloading checksum from ${checksumUrl}...`)
    let expectedChecksum = ''
    try {
      const checksumRes = await axios.get(checksumUrl, {
        responseType: 'text',
        timeout: 10000, // Shorter timeout for small checksum file
        validateStatus: (status) => status === 200,
      })
      // Checksum file has the format: HASH  FILENAME
      // We extract the hash (the first part) after trimming whitespace.
      const checksumContent = checksumRes.data.trim()
      expectedChecksum = checksumContent.split(/\s+/)[0] // Split on whitespace and take the first part
      if (!/^[a-f0-9]{64}$/i.test(expectedChecksum)) {
        // Basic validation for SHA256 hash format
        throw new Error(
          `Invalid checksum format received or parsed: ${expectedChecksum} from content [${checksumContent}]`,
        )
      }
      bamlOutputChannel.appendLine(`Expected checksum: ${expectedChecksum}`)
    } catch (checksumError: any) {
      bamlOutputChannel.appendLine(`ERROR: Failed to download or parse checksum file: ${checksumError.message}`)
      if (axios.isAxiosError(checksumError) && checksumError.response?.status === 404) {
        bamlOutputChannel.appendLine(`WARN: Checksum file not found at ${checksumUrl}. Cannot verify integrity.`)
        // Decide whether to proceed without checksum or fail. Let's fail for safety.
        throw new Error(`Checksum file not found at ${checksumUrl}. Aborting download for security.`)
      } else {
        throw checksumError // Re-throw other checksum errors
      }
    }

    // 3. Calculate hash of downloaded file
    bamlOutputChannel.appendLine(`Calculating SHA256 hash for ${compressedFileTempPath}...`)
    const fileBuffer = await fs.promises.readFile(compressedFileTempPath)
    const calculatedHash = crypto.createHash('sha256').update(fileBuffer).digest('hex')
    bamlOutputChannel.appendLine(`Calculated checksum: ${calculatedHash}`)

    // 4. Compare hashes
    if (calculatedHash !== expectedChecksum) {
      throw new Error(
        `Checksum mismatch! Expected ${expectedChecksum}, got ${calculatedHash}. File may be corrupted or tampered with.`,
      )
    }
    bamlOutputChannel.appendLine('Checksum verification successful!')
    checksumVerified = true

    // 5. Extract the verified file
    bamlOutputChannel.appendLine('Proceeding with extraction...')
    await _extractFile(compressedFileTempPath, extension, targetFileName, installPath, bamlOutputChannel)

    bamlOutputChannel.appendLine(`Successfully downloaded, verified, and extracted BAML CLI to: ${targetFullPath}`)

    // Reset backoff state on complete success
    downloadBackoffState.delete(cliVersion.version)
    bamlOutputChannel.appendLine(`Reset download backoff state for version ${cliVersion.version}.`)

    return targetFullPath // Return the path to the final executable
  } catch (error: any) {
    bamlOutputChannel.appendLine(
      `ERROR: Failed during download/verification/extraction process for BAML CLI v${cliVersion.version}: ${error.message}`,
    )
    // Log more details for debugging
    if (error.stack) {
      bamlOutputChannel.appendLine(`Stack trace: ${error.stack}`)
    }

    // Update backoff state on any failure during the process
    const now = Date.now()
    const state = downloadBackoffState.get(cliVersion.version) ?? { failureCount: 0, lastAttemptTimestamp: 0 }
    state.failureCount = state.failureCount + 1
    state.lastAttemptTimestamp = now

    // Reset failure count if it gets too high, allowing retries after a long pause
    if (state.failureCount > MAX_FAILURE_COUNT_BEFORE_RESET) {
      bamlOutputChannel.appendLine(
        `Download failure count for ${cliVersion.version} reached ${state.failureCount}. Resetting count but maintaining backoff timestamp.`,
      )
      state.failureCount = 1 // Reset to 1, not 0, to maintain backoff
    }

    downloadBackoffState.set(cliVersion.version, state)
    bamlOutputChannel.appendLine(
      `Updated download backoff state for version ${cliVersion.version}: count=${state.failureCount}, lastAttempt=${new Date(
        state.lastAttemptTimestamp,
      ).toISOString()}`,
    )

    // Specific logging (already done by the error message itself)
    if (axios.isAxiosError(error)) {
      // Log Axios specific details if needed
      if (error.response) {
        bamlOutputChannel.appendLine(`Axios Error: Status ${error.response.status} - ${error.response.statusText}`)
      } else if (error.request) {
        bamlOutputChannel.appendLine('Axios Error: No response received from server.')
      } else {
        bamlOutputChannel.appendLine(`Axios Error: Request setup failed - ${error.message}`)
      }
    }

    return null // Indicate failure
  } finally {
    // 6. Clean up the temporary downloaded file if it exists
    if (downloadSucceeded) {
      try {
        bamlOutputChannel.appendLine(`Cleaning up temporary file: ${compressedFileTempPath}`)
        await fs.promises.unlink(compressedFileTempPath)
        bamlOutputChannel.appendLine(`Temporary file deleted.`)
      } catch (cleanupError: any) {
        // Log cleanup error but don't fail the whole operation because of it
        bamlOutputChannel.appendLine(
          `WARN: Failed to delete temporary download file ${compressedFileTempPath}: ${cleanupError.message}`,
        )
      }
    }
  }
}

/**
 * Finds the path to the bundled CLI executable, considering dev overrides and Linux fallbacks.
 * Internal helper.
 * @param context Extension context.
 * @returns The absolute path to the bundled CLI if found and valid, otherwise null.
 */
function _getBundledCliPath(context: ExtensionContext): string | null {
  const platform = os.platform()
  const executableName = _getExecutableName()
  const targetTriple = _getTargetTriple()

  console.log(`Trying to find bundled CLI for triple: ${targetTriple}`)

  // 1. Check Development Override Path first
  const devServerPath = context.asAbsolutePath(path.join('vscode', 'server', executableName))
  if (fs.existsSync(devServerPath)) {
    console.log('Found bundled CLI at development override path:', devServerPath)
    if (_ensureExecutablePermissions(devServerPath)) {
      return devServerPath
    } else {
      console.error(`Development override CLI found but failed to set permissions: ${devServerPath}`)
      // Don't proceed with this path if permissions fail
      return null
    }
  }

  // 2. Check Standard Bundled Path if targetTriple is known
  if (targetTriple) {
    const primaryBundledPath = context.asAbsolutePath(path.join('vscode', 'server', targetTriple, executableName))
    console.log(`Checking standard bundled path: ${primaryBundledPath}`)

    if (fs.existsSync(primaryBundledPath)) {
      console.log('Found bundled CLI at standard path:', primaryBundledPath)
      if (_ensureExecutablePermissions(primaryBundledPath)) {
        return primaryBundledPath
      } else {
        console.error(`Standard bundled CLI found but failed to set permissions: ${primaryBundledPath}`)
        // Fall through to potentially check MUSL if Linux
      }
    }

    // 3. Linux MUSL Fallback (only if primary GNU path failed or permissions failed)
    if (platform === 'linux' && targetTriple.endsWith('-gnu')) {
      const muslTargetTriple = targetTriple.replace('-gnu', '-musl')
      const muslBundledPath = context.asAbsolutePath(path.join('vscode', 'server', muslTargetTriple, executableName))
      console.log(`Checking Linux MUSL fallback path: ${muslBundledPath}`)
      if (fs.existsSync(muslBundledPath)) {
        console.log('Found bundled CLI at MUSL fallback path:', muslBundledPath)
        if (_ensureExecutablePermissions(muslBundledPath)) {
          return muslBundledPath
        } else {
          console.error(`MUSL fallback CLI found but failed to set permissions: ${muslBundledPath}`)
        }
      }
    }
  } else {
    console.warn('Target triple could not be determined, cannot check standard bundled path.')
  }

  // 4. If none of the above worked
  console.log('Bundled CLI executable not found in expected locations.')
  return null
}

/**
 * Helper to set executable permissions (non-Windows). Returns true on success/Windows, false on failure.
 */
function _ensureExecutablePermissions(filePath: string): boolean {
  if (os.platform() !== 'win32') {
    try {
      fs.chmodSync(filePath, 0o755) // Use sync for simplicity here
      console.log(`Ensured executable permissions for: ${filePath}`)
      return true
    } catch (err: any) {
      console.error(`Failed to chmod server executable at ${filePath}: ${err}`)
      window.showErrorMessage(
        `Failed to set permissions for BAML Language Server executable. Please check file permissions at ${filePath}`,
      )
      return false
    }
  }
  return true // No-op on Windows, always succeeds
}

/**
 * Resolves the absolute path to the BAML CLI executable, handling bundled vs downloaded.
 * Tries to use bundled if versions match, otherwise checks for downloaded, or triggers download.
 * Public API for index.ts.
 *
 * @param context Extension context.
 * @param requestedVersion The desired version of the CLI (e.g., from notification or package.json).
 * @returns The absolute path to the executable, or null if resolution fails.
 */
export async function resolveCliPath(
  context: ExtensionContext,
  requestedVersion: string,
  bamlOutputChannel: OutputChannel,
): Promise<string | null> {
  bamlOutputChannel.appendLine(`Resolving CLI path for version: ${requestedVersion}`)
  const bundledVersion = packageJson.version as string

  // 1. Check if requested version matches bundled version
  if (semver.valid(requestedVersion) && semver.valid(bundledVersion) && semver.eq(requestedVersion, bundledVersion)) {
    console.log(`Requested version (${requestedVersion}) matches bundled version (${bundledVersion}).`)
    const bundledPath = _getBundledCliPath(context)
    if (bundledPath) {
      bamlOutputChannel.appendLine(`Using bundled CLI path: ${bundledPath}`)
      return bundledPath
    } else {
      bamlOutputChannel.appendLine(
        'WARN: Bundled version matches, but executable not found/accessible. Will attempt to use/download specific version.',
      )
      // Fall through to download logic as a backup
    }
  } else {
    bamlOutputChannel.appendLine(
      `Requested version (${requestedVersion}) does not match bundled version (${bundledVersion}).`,
    )
  }

  // 2. Check if the specific requested version is already downloaded
  const cliVersionMeta: CliVersion = {
    architecture: os.arch(),
    platform: os.platform(),
    version: requestedVersion,
  }
  const expectedDownloadedPath = downloadedCliPath(context, cliVersionMeta)

  if (await checkIfDownloadedCliExists(context, cliVersionMeta)) {
    bamlOutputChannel.appendLine(
      `Found existing downloaded CLI for version ${requestedVersion}: ${expectedDownloadedPath}`,
    )
    // Verify permissions just in case
    if (_ensureExecutablePermissions(expectedDownloadedPath)) {
      return expectedDownloadedPath
    } else {
      bamlOutputChannel.appendLine(
        `ERROR: Downloaded CLI exists but permissions failed for ${expectedDownloadedPath}. Attempting re-download.`,
      )
      // Fall through to download logic to try and fix it.
    }
  }

  // Check backoff state before attempting download
  const backoffInfo = downloadBackoffState.get(requestedVersion)
  if (backoffInfo) {
    const { failureCount, lastAttemptTimestamp } = backoffInfo
    const backoffDelay = Math.min(INITIAL_BACKOFF_DELAY_MS * Math.pow(2, failureCount - 1), MAX_BACKOFF_DELAY_MS)
    const nextAttemptTime = lastAttemptTimestamp + backoffDelay

    if (Date.now() < nextAttemptTime) {
      const waitTimeMinutes = Math.ceil((nextAttemptTime - Date.now()) / (60 * 1000))
      bamlOutputChannel.appendLine(
        `Download for version ${requestedVersion} failed previously (${failureCount} times). Backoff active. Will not attempt download for another ${waitTimeMinutes} minutes.`,
      )
      return null // Skip download due to backoff
    } else {
      bamlOutputChannel.appendLine(
        `Backoff period for version ${requestedVersion} has elapsed. Proceeding with download attempt.`,
      )
    }
  }

  // Check if a download for this version is already in progress
  if (downloadsInProgress.has(requestedVersion)) {
    bamlOutputChannel.appendLine(
      `Download for version ${requestedVersion} is already in progress. Skipping duplicate request.`,
    )
    return null
  }

  // 3. Download the specific requested version
  let downloadedPath: string | null = null
  try {
    // Acquire lock
    downloadsInProgress.add(requestedVersion)
    bamlOutputChannel.appendLine(`Attempting to download CLI version ${requestedVersion}... [Lock acquired]`)

    // Show progress during download attempt
    downloadedPath = await window.withProgress(
      {
        location: ProgressLocation.Notification,
        cancellable: false,
        title: `Downloading BAML Language Server v${requestedVersion}`,
      },
      async (progress) => {
        // Pass the output channel to downloadCli
        return await downloadCli(context, cliVersionMeta, bamlOutputChannel)
      },
    )
  } catch (error) {
    // Catch any unexpected errors during the withProgress or downloadCli call itself
    // although downloadCli has its own internal catch
    const errorMsg = error instanceof Error ? error.message : String(error)
    bamlOutputChannel.appendLine(
      `ERROR: Unexpected error during download process initiation for ${requestedVersion}: ${errorMsg}`,
    )
    downloadedPath = null // Ensure path is null on error
  } finally {
    // Release lock
    downloadsInProgress.delete(requestedVersion)
    bamlOutputChannel.appendLine(`Download attempt for ${requestedVersion} finished. [Lock released]`)
  }

  if (downloadedPath) {
    window.showInformationMessage(`BAML CLI v${requestedVersion} downloaded successfully!`)
    return downloadedPath
  } else {
    // Error message should have been logged to the Baml output channel by downloadCli
    bamlOutputChannel.appendLine(
      `ERROR: Failed to resolve CLI path for version ${requestedVersion} after download attempt.`,
    )
    return null // Indicate failure
  }
}
