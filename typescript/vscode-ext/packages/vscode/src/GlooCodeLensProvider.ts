import * as vscode from 'vscode'

export class GlooCodeLensProvider implements vscode.CodeLensProvider {
  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    const codeLenses: vscode.CodeLens[] = []

    let insideTestGroupBlock = false

    const braceCount = 0
    for (let line = 0; line < document.lineCount; line++) {
      const lineText = document.lineAt(line).text

      // Detect the start of a test group
      if (lineText.includes('@test_group')) {
        insideTestGroupBlock = true
      }

      // If we're inside a test group and we find an @input line, add a code lens
      if (insideTestGroupBlock && lineText.includes('@input')) {
        const range = new vscode.Range(line, 0, line, lineText.length)
        const command: vscode.Command = {
          title: '▶️ Run Test',
          command: 'extension.runGlooTest',
          arguments: [document.uri],
        }
        codeLenses.push(new vscode.CodeLens(range, command))
      }

      // Detect the end of the test group block
      if (insideTestGroupBlock && lineText.trim() === '}') {
        insideTestGroupBlock = false
      }
    }

    return codeLenses
  }
}
