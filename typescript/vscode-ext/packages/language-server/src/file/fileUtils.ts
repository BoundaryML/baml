import * as fs from 'fs'
import * as path from 'path'
import { TextDocument } from 'vscode-languageserver-textdocument'
import { URI } from 'vscode-uri'

export function findTopLevelParent(filePath: string) {
  let currentPath = filePath
  let parentDir: string | null = null

  while (currentPath !== path.parse(currentPath).root) {
    currentPath = path.dirname(currentPath)
    if (path.basename(currentPath) === 'baml_src') {
      parentDir = currentPath
      break
    }
  }

  if (parentDir !== null) {
    return parentDir
  }
  return null
}

/**
 * Non-recursively gathers files with .baml or .json extensions from a given directory,
 * avoiding processing the same directory more than once.
 *
 * @param {vscode.Uri} uri - The URI of the directory to search.
 * @param {boolean} debug - Flag to enable debug logging.
 * @returns {string[]} - An array of file URIs.
 */
export function gatherFiles(rootPath: string, debug = false): URI[] {
  const visitedDirs = new Set<string>()
  const dirStack: URI[] = []
  const addDir = (dir: URI) => {
    if (!visitedDirs.has(dir.toString())) {
      dirStack.push(dir)
      visitedDirs.add(dir.toString())
    }
  }

  addDir(URI.parse(rootPath))

  const fileList: URI[] = []

  const MAX_DIRS = 1000
  let iterations = 0

  while (dirStack.length > 0) {
    if (iterations > MAX_DIRS) {
      console.error(`Max directory limit reached (${MAX_DIRS})`)
      throw new Error(`Directory failed to load after ${iterations} iterations`)
    }
    iterations++

    const currentUri = dirStack.pop()!
    const dirPath = currentUri.fsPath

    try {
      const files = fs.readdirSync(dirPath)

      files.forEach((file) => {
        const filePath = path.join(dirPath, file)
        const fileUri = URI.file(filePath)
        const fileStat = fs.statSync(filePath)

        if (fileStat.isDirectory()) {
          addDir(fileUri)
        } else if (filePath.endsWith('.baml') || filePath.endsWith('.json')) {
          fileList.push(fileUri)
        }
      })
    } catch (error) {
      console.error(`Error reading directory ${dirPath}: ${(error as any).message}`)
      throw error
    }
  }

  return fileList
}

export function convertToTextDocument(filePath: URI): TextDocument {
  const fileContent = fs.readFileSync(filePath.fsPath, 'utf-8')
  const fileExtension = path.extname(filePath.fsPath)
  return TextDocument.create(filePath.toString(), fileExtension === '.baml' ? 'baml' : 'json', 1, fileContent)
}
