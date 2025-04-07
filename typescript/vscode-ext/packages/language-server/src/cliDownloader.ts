import { createWriteStream } from 'fs'
import fs from 'fs/promises'
import path from 'path'
import os from 'os'
import zlib from 'zlib'
import * as tar from 'tar'
import https from 'https'
import extractZip from 'extract-zip'
import axios from 'axios'

// TODO: This is a draft release for testing.
const BASE_URL = 'https://github.com/BoundaryML/baml/releases/download/untagged-c52d304b99ce91cdc208'

// TODO: $HOME/.baml for Linux, figure out other platforms.
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
  switch (platform) {
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

export async function downloadCli(platform: string, architecture: string, version: string): Promise<string> {
  // TODO: Validate params.
  architecture = getReleaseArchitecture(architecture)
  platform = getReleasePlatform(platform)
  const extension = getCliCompressedFileExtension(platform)

  // Filenames.
  const binaryFileName = `baml-cli-${version}-${architecture}-${platform}`
  const compressedFileName = `${binaryFileName}.${extension}`

  // Github release download URL.
  // const url = `${BASE_URL}/${compressedFileName}`

  // TODO: Testing
  const url =
    'https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz'

  // Full path on disk of the CLI binary.
  const binaryFilePath = path.join(INSTALL_PATH, binaryFileName)

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

  // Download the compressed file, extract and write the binary.
  // TODO: Check zip vs tar.gz.
  res.data.pipe(
    tar.extract(
      {
        cwd: INSTALL_PATH,
        onReadEntry: (entry) => entry.path = binaryFileName
      },
      ['ripgrep-14.1.1-x86_64-unknown-linux-musl/rg'], // TODO: Change this to ['./baml-cli']
    ),
  )

  // TODO: Zip files
  // const unzip = zlib.createGunzip()
  // unzip.pipe(createWriteStream(path.join(INSTALL_PATH, binaryFileName)))
  // await extractZip(compressedFilePath, { dir: INSTALL_PATH })

  return binaryFilePath
}
