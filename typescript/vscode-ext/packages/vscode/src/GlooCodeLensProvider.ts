import { ParserDatabase } from '@baml/common'
import * as vscode from 'vscode'

export class GlooCodeLensProvider implements vscode.CodeLensProvider {
  private db: ParserDatabase | undefined

  public setDB(db: ParserDatabase) {
    this.db = db
  }

  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = []

    if (!this.db) {
      return codeLenses
    }

    const functionNames = this.db.functions
      .filter((x) => x.name.source_file === document.uri.toString())
      .map((f) => f.name)

    functionNames.forEach((name) => {
      const range = new vscode.Range(document.positionAt(name.start), document.positionAt(name.end))
      const command: vscode.Command = {
        title: '▶️ Open Playground',
        command: 'baml.openBamlPanel',
        arguments: [
          {
            functionName: name.value,
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
      .filter((x) => x.source_file === document.uri.toString())

    implNames.forEach((name) => {
      codeLenses.push(
        new vscode.CodeLens(new vscode.Range(document.positionAt(name.start), document.positionAt(name.end)), {
          title: '▶️ Open Playground',
          command: 'baml.openBamlPanel',
          arguments: [
            {
              functionName: name.function,
              implName: name.value,
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
                functionName: name.function,
                implName: name.value,
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
      .filter((x) => x.source_file === document.uri.toString())
    testCases.forEach((name) => {
      const range = new vscode.Range(document.positionAt(name.start), document.positionAt(name.end))
      const command: vscode.Command = {
        title: '▶️ Open Playground',
        command: 'baml.openBamlPanel',
        arguments: [
          {
            functionName: name.function,
            testCaseName: name.value,
          },
        ],
      }
      codeLenses.push(new vscode.CodeLens(range, command))
    })

    return codeLenses
  }
}

export default new GlooCodeLensProvider()
