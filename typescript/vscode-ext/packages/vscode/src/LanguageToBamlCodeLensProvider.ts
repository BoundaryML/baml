import * as vscode from 'vscode'
import { getBAMLFunctions } from './plugins/language-server'

export class LanguageToBamlCodeLensProvider implements vscode.CodeLensProvider {
  public async provideCodeLenses(document: vscode.TextDocument): Promise<vscode.CodeLens[]> {
    if (document.languageId === 'python' || document.languageId === 'typescript') {
      return this.getCodeLenses(document)
    }

    const codeLenses: vscode.CodeLens[] = []
    return codeLenses
  }

  private async getCodeLenses(document: vscode.TextDocument): Promise<vscode.CodeLens[]> {
    const codeLenses: vscode.CodeLens[] = []

    const text = document.getText()

    // Check for baml_client import
    if (!text.includes('baml_client')) {
      return codeLenses
    }

    // Match all occurrences of baml function calls
    const functionCalls = [...text.matchAll(/(baml|b)(\.[a-zA-Z0-9_]+)+/g)]
    if (functionCalls.length === 0) {
      return codeLenses
    }

    let bamlFunctions: any

    try {
      // Get BAML functions in this project
      const response = await getBAMLFunctions()
      if (!response) {
        return codeLenses
      }
      bamlFunctions = JSON.parse(response)
    } catch (e) {
      console.error(`Error fetching BAML functions: ${e}`)
      return codeLenses
    }

    // Iterate over each function call
    functionCalls.forEach((match) => {
      const call = match[0]
      const position = match.index ?? 0
      const functionArr = call.split('.')
      const functionName = functionArr[functionArr.length - 1]

      // Find the corresponding function definition in bamlFunctions
      const functionDef = bamlFunctions.find((f: any) => f.name === functionName)
      if (functionDef) {
        const range = new vscode.Range(document.positionAt(position), document.positionAt(position + call.length))

        // Placeholder function to parse arguments into a readable format

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

    return codeLenses
  }
}

export default new LanguageToBamlCodeLensProvider()
