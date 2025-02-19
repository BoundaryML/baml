import { existsSync } from 'fs'
import path from 'path'
import { readFile } from 'fs/promises'
import { compile as compileGitignore } from 'gitignore-parser'

/**
 * Reads and combines .gitignore patterns from the current directory up to the git root.
 * Filters out any patterns that would ignore baml_client.
 * @param startPath The directory path to start searching from
 * @returns A function that takes a file path and returns whether it should be ignored
 */
export async function readGitignorePatterns(startPath: string): Promise<(path: string) => boolean> {
  let currentPath = startPath
  let foundGitRoot = false
  let combinedGitignore = ''

  while (!foundGitRoot && currentPath !== '/') {
    try {
      // Check if we've hit the git root
      if (existsSync(path.join(currentPath, '.git'))) {
        foundGitRoot = true
      }

      // Read .gitignore if it exists
      const gitignorePath = path.join(currentPath, '.gitignore')
      if (existsSync(gitignorePath)) {
        const content = await readFile(gitignorePath, 'utf8')
        // Filter out any patterns that would ignore baml_client
        const filteredContent = content
          .split('\n')
          .filter((line) => !line.includes('baml_client'))
          .join('\n')
        combinedGitignore += filteredContent + '\n'
      }

      // Move up one directory
      currentPath = path.dirname(currentPath)
    } catch (error) {
      console.warn(`Error reading .gitignore in ${currentPath}:`, error)
      break
    }
  }

  // If no gitignore files found, return a function that denies nothing
  if (!combinedGitignore.trim()) {
    return () => false
  }

  // Parse the combined gitignore content
  const gitignoreParser = compileGitignore(combinedGitignore)
  return (filePath: string) => gitignoreParser.denies(filePath)
}
