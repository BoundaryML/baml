import crypto from 'crypto'
import { window, workspace } from 'vscode'

/**
 * Get a unique identifier for the project by hashing
 * the directory with `schema.prisma`
 */
export async function getProjectHash(): Promise<string> {
  let projectPath = await getSchemaPath()
  projectPath = projectPath || process.cwd() // Default to cwd if the schema couldn't be found
  console.log('projectPath: ' + projectPath)

  return crypto.createHash('sha256').update(projectPath).digest('hex').substring(0, 8)
}

async function getSchemaPath(): Promise<string | null> {
  // try the currently open document
  const schemaPath = window.activeTextEditor?.document.fileName;
  if (schemaPath && schemaPath.includes('baml_src')) {
    return schemaPath.substring(0, schemaPath.indexOf('baml_src') + 'baml_src'.length);
  }

  // try the workspace
  const fileInWorkspace = await workspace.findFiles('**/baml_src/**/*', '**/node_modules/**');
  if (fileInWorkspace.length !== 0) {
    const fullPath = fileInWorkspace[0].toString();
    return fullPath.substring(0, fullPath.indexOf('baml_src') + 'baml_src'.length);
  }

  return null;
}