import { ParserDatabase } from '@baml/common'
import * as vscode from 'vscode'
import { URI } from 'vscode-languageclient'

export class GlooCodeLensProvider implements vscode.CodeLensProvider {
  private db: ParserDatabase | undefined
  private path: string | undefined

  public setDB(path: string, db: ParserDatabase) {
    this.path = path
    this.db = db
  }

  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    if (document.languageId === 'baml') {
      return this.getBamlCodeLenses(document)
    } else if (document.languageId === 'json') {
      return this.getBamlCodeLenses(document)
    } else if (document.languageId === 'python') {
      return this.getPythonCodeLenses(document)
    } else {
      return []
    }
  }

  private getPythonCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = []

    if (!this.db || !this.path) {
      return codeLenses
    }

    // Check if we imported baml_client in this file
    let text = document.getText()
    const bamlImport = text.includes('import baml_client') || text.includes('from baml_client')
    if (!bamlImport) {
      return codeLenses
    }

    // By convention we only import baml as baml or b so then look for all
    // baml.function_name() or b.function_name() calls and also get the range
    const functionCalls = [...text.matchAll(/(baml|b)\.[a-zA-Z0-9_]+/g)]

    // For each function call, find the function name and then find the
    // function in the db
    functionCalls.forEach((match) => {
      const call = match[0]
      const position = match.index ?? 0
      // get line number
      const line = document.positionAt(position)

      const functionName = call.split('.')[1]
      const functionDef = this.db?.functions.find((f) => f.name.value === functionName)
      if (functionDef) {
        const range = new vscode.Range(
          document.positionAt(position),
          document.positionAt(position + functionName.length),
        )

        const fromArgType = (arg: ParserDatabase['functions'][0]['input']) => {
          if (arg.arg_type === 'positional') {
            return `${arg.type}`
          } else {
            return arg.values.map((v) => `${v.name.value}: ${v.type}`).join(', ')
          }
        }
        const command: vscode.Command = {
          title: `(${fromArgType(functionDef.input)}) -> ${fromArgType(functionDef.output)}`,
          tooltip: 'Open in BAML',
          command: 'baml.jumpToDefinition',
          arguments: [
            {
              sourceFile: functionDef.name.source_file,
              name: functionName,
            },
          ],
        }
        codeLenses.push(new vscode.CodeLens(range, command))
      }
    })

    return codeLenses
  }

  private getBamlCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = []

    if (!this.db || !this.path) {
      return codeLenses
    }

    const functionNames = this.db.functions.filter((x) => x.name.source_file === document.uri.fsPath).map((f) => f.name)

    functionNames.forEach((name) => {
      const range = new vscode.Range(document.positionAt(name.start), document.positionAt(name.end))
      const command: vscode.Command = {
        title: '▶️ Open Playground',
        command: 'baml.openBamlPanel',
        arguments: [
          {
            projectId: this.path,
            functionName: name.value,
            showTests: true,
          },
        ],
      }
      codeLenses.push(new vscode.CodeLens(range, command))
    })

    const implNames = this.db.functions
      .flatMap((f) =>
        f.impls.map((i) => {
          return {
            value: i.name.value,
            start: i.name.start,
            end: i.name.end,
            source_file: i.name.source_file,
            prompt_key: i.prompt_key,
            function: f.name.value,
          }
        }),
      )
      .filter((x) => x.source_file === document.uri.fsPath)

    implNames.forEach((name) => {
      codeLenses.push(
        new vscode.CodeLens(new vscode.Range(document.positionAt(name.start), document.positionAt(name.end)), {
          title: '▶️ Open Playground',
          command: 'baml.openBamlPanel',
          arguments: [
            {
              projectId: this.path,
              functionName: name.function,
              implName: name.value,
              showTests: true,
            },
          ],
        }),
      )
      codeLenses.push(
        new vscode.CodeLens(
          new vscode.Range(document.positionAt(name.prompt_key.start), document.positionAt(name.prompt_key.end)),
          {
            title: '▶️ Open Live Preview',
            command: 'baml.openBamlPanel',
            arguments: [
              {
                projectId: this.path,
                functionName: name.function,
                implName: name.value,
                showTests: false,
              },
            ],
          },
        ),
      )
    })

    const testCases = this.db.functions
      .flatMap((f) =>
        f.test_cases.map((t) => {
          return {
            value: t.name.value,
            start: t.name.start,
            end: t.name.end,
            source_file: t.name.source_file,
            function: f.name.value,
          }
        }),
      )
      .filter((x) => x.source_file === document.uri.fsPath)
    testCases.forEach((name) => {
      const range = new vscode.Range(document.positionAt(name.start), document.positionAt(name.end))
      const command: vscode.Command = {
        title: '▶️ Open Playground',
        command: 'baml.openBamlPanel',
        arguments: [
          {
            projectId: this.path,
            functionName: name.function,
            testCaseName: name.value,
            showTests: true,
          },
        ],
      }
      codeLenses.push(new vscode.CodeLens(range, command))
    })

    return codeLenses
  }
}

export default new GlooCodeLensProvider()
