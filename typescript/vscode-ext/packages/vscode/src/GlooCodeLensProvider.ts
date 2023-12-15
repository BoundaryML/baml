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
