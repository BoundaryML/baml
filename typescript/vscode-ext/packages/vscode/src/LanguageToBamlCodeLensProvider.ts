import * as vscode from 'vscode'
import { getBAMLFunctions } from './plugins/language-server'

// Helper function to create regex patterns for matching BAML function calls
function createBamlPatterns() {
  return [
    // React hooks pattern - matches both direct and namespace imports with parameters
    new RegExp('(?:[a-zA-Z0-9_]+\\.)?use([A-Z][a-zA-Z0-9_]*)\\s*\\([^)]*\\)', 'g'),
    // Direct BAML client pattern - matches capitalized function names only
    new RegExp('(?:^|\\s)(?:baml|b)\\.([A-Z][a-zA-Z0-9_]*)(?:\\s|\\(|$)', 'g'),
  ]
}

export class LanguageToBamlCodeLensProvider implements vscode.CodeLensProvider {
  public async provideCodeLenses(document: vscode.TextDocument): Promise<vscode.CodeLens[]> {
    if (
      document.languageId === 'python' ||
      document.languageId === 'typescript' ||
      document.languageId === 'typescriptreact'
    ) {
      return this.getCodeLenses(document)
    }

    const codeLenses: vscode.CodeLens[] = []
    return codeLenses
  }

  private async getCodeLenses(document: vscode.TextDocument): Promise<vscode.CodeLens[]> {
    const codeLenses: vscode.CodeLens[] = []

    const text = document.getText()

    // Check for baml_client imports or direct baml usage
    if (!text.includes('baml_client') && !text.includes('baml.') && !text.includes('b.')) {
      return codeLenses
    }

    // Get BAML functions in this project
    const bamlFunctions = await getBAMLFunctions()

    // Match all occurrences of BAML function calls using both patterns
    const patterns = createBamlPatterns()

    for (const pattern of patterns) {
      const functionCalls = [...text.matchAll(pattern)]

      // Iterate over each function call
      functionCalls.forEach((match) => {
        const call = match[0] // Full match including namespace if present
        const functionName = match[1] // Both patterns now capture the function name in group 1
        const position = match.index ?? 0

        // Find the corresponding function definition in bamlFunctions
        const functionDef = bamlFunctions.find((f: any) => f.name === functionName)
        if (functionDef) {
          const range = new vscode.Range(document.positionAt(position), document.positionAt(position + call.length))

          // Create the command for the code lens
          const command: vscode.Command = {
            title: `▶️ Open ${functionDef.name} in BAML`,
            tooltip: 'Jump to definition',
            command: 'baml.jumpToDefinition',
            arguments: [
              {
                file_path: functionDef.span.file_path,
                start: functionDef.span.start,
                end: functionDef.span.end,
              },
            ],
          }

          codeLenses.push(new vscode.CodeLens(range, command))
        }
      })
    }

    return codeLenses
  }
}

export default new LanguageToBamlCodeLensProvider()
