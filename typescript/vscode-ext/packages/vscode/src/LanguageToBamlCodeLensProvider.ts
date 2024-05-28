import type { ParserDatabase } from '@baml/common'
import * as vscode from 'vscode'
import { URI } from 'vscode-languageclient'
import { type LanguageClient } from 'vscode-languageclient/node'
import { getBAMLFunctions } from './plugins/language-server'
let client: LanguageClient

export class LanguageToBamlCodeLensProvider implements vscode.CodeLensProvider {
  private db: ParserDatabase | undefined

  // public setDB(path: string, db: ParserDatabase) {
  //   this.path = path
  //   this.db = db
  // }

  public async provideCodeLenses(document: vscode.TextDocument): Promise<vscode.CodeLens[]> {
    console.log('providing code lenses')
    try {
      if (document.languageId === 'python') {
        return this.getPythonCodeLenses(document)
      } else if (document.languageId === 'typescript') {
        return this.getTypeScriptCodeLenses(document)
      }
    } catch (e) {
      console.log('Error providing code lenses' + JSON.stringify(e, null, 2))
    }
    const codeLenses: vscode.CodeLens[] = []
    return codeLenses
  }

  private getTypeScriptCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = []

    return codeLenses
  }

  private async getPythonCodeLenses(document: vscode.TextDocument): Promise<vscode.CodeLens[]> {
    console.log('Getting Python code lenses')
    const codeLenses: vscode.CodeLens[] = []

    const text = document.getText()

    // Check for baml_client import
    if (!text.includes('baml_client')) {
      return codeLenses
    }

    // Match all occurrences of baml function calls
    const functionCalls = [...text.matchAll(/(baml|b)\.[a-zA-Z0-9_]+/g)]
    if (functionCalls.length === 0) {
      return codeLenses
    }

    let bamlFunctions: any
    console.log('Making client request')
    try {
      // Get BAML functions in this project
      const response = await getBAMLFunctions()
      if (!response) {
        return codeLenses
      }
      bamlFunctions = JSON.parse(response)
      console.log('BAML functions received')
      console.log(`bamlFunctions: ${JSON.stringify(bamlFunctions, null, 2)}`)
    } catch (e) {
      console.error(`Error fetching BAML functions: ${e}`)
      return codeLenses
    }

    if (bamlFunctions) {
      bamlFunctions.forEach((func: { name: string }) => {
        console.log(func.name)
      })
    } else {
      console.log('No functions to display.')
    }

    // Iterate over each function call
    functionCalls.forEach((match) => {
      const call = match[0]
      const position = match.index ?? 0
      const functionName = call.split('.')[1]
      console.log(`Current iterated function name: ${functionName}`)
      // Find the corresponding function definition in bamlFunctions
      const functionDef = bamlFunctions.find((f: any) => f.name === functionName)
      if (functionDef) {
        console.log(`Found function definition: ${functionDef}`)
        const range = new vscode.Range(document.positionAt(position), document.positionAt(position + call.length))

        // Placeholder function to parse arguments into a readable format
        const formatArguments = (args: any) => args.map((arg: any) => `${arg.name}: ${arg.type}`).join(', ')

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
