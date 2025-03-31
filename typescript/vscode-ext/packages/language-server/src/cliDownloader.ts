import { createWriteStream } from 'fs'
import fs from 'fs/promises'
import path from 'path'
import os from 'os'
import zlib from 'zlib'
import https from 'https'
import extractZip from 'extract-zip'

// const BASE_URL = 'https://github.com/BoundaryML/baml/archive/refs/tags'

// Greg test binaries. (Github makes this avaialable temporarily).
const BASE_URL = 'https://productionresultssa17.blob.core.windows.net/actions-results/416b3245-9c0e-455e-ad03-06d303c50a91/workflow-job-run-061a9379-8e44-5d89-a290-172f1bb22565/artifacts/ba0b3c839a2939ecd0c2dda1b78093fece8b9c1cb58d91bbdcab7771fe1b8239.zip?rscd=attachment%3B+filename%3D%22baml-cli-x86_64-unknown-linux-gnu.zip%22&se=2025-03-31T14%3A00%3A24Z&sig=ivrkTl7apHUMLqKph5n1Jhp4ZQIonXzbAfM58EV3FZc%3D&ske=2025-04-01T00%3A00%3A27Z&skoid=ca7593d4-ee42-46cd-af88-8b886a2f84eb&sks=b&skt=2025-03-31T12%3A00%3A27Z&sktid=398a6654-997b-47e9-b12b-9515b896b4de&skv=2025-01-05&sp=r&spr=https&sr=b&st=2025-03-31T13%3A50%3A19Z&sv=2025-01-05'

// $HOME/.baml
const INSTALL_PATH = path.join(os.homedir(), '.baml')

export async function downloadCli(platform: string, architecture: string, version: string): Promise<string> {
  console.debug('downloadCli', { platform, architecture, version })
  return path.join(INSTALL_PATH, `baml-cli-${platform}-${architecture}-${version}`)

  // TODO: Validate params.

  // const url = `${BASE_URL}/${version}/baml-cli-${platform}-${architecture}-${version}.tar.gz`
  // const url = "https://github.com/BoundaryML/baml/archive/refs/tags/0.81.1.zip"
  const url = BASE_URL

  console.log('downloading', url)
  const req = https.get(url, async (res) => {
    try {
      console.log('access', INSTALL_PATH)
      await fs.access(INSTALL_PATH)
    } catch (e) {
      console.log('mkdir', INSTALL_PATH)
      await fs.mkdir(INSTALL_PATH, { recursive: true })
    }

    try {
      console.log('access', INSTALL_PATH)
      await fs.access(INSTALL_PATH)
    } catch (e) {
      console.log('mkdir', INSTALL_PATH)
      await fs.mkdir(INSTALL_PATH, { recursive: true })
    }

    const compressedFilePath = path.join(INSTALL_PATH, `baml-cli-${platform}-${architecture}-${version}.zip`)
    res.pipe(createWriteStream(compressedFilePath))

    console.log('decompress', compressedFilePath)
    // const unzip = zlib.createGunzip()
    // res.pipe(unzip).pipe(filePath)
    await extractZip(compressedFilePath, { dir: INSTALL_PATH })

    await fs.unlink(compressedFilePath)
  })

  req.on('error', (err) => {
    console.error('Request error:', err)
  })
}
