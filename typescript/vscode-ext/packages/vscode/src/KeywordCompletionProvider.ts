import * as vscode from 'vscode'

const keywords = [
  '@test_group',
  '@input',
  '@alias',
  '@description',
  '@skip',
  '@stringify',
  '@client',
  '@method',
  '@lang',
  '@provider',
]

const commitCharacters = [
  'a',
  'b',
  'c',
  'd',
  'e',
  'f',
  'g',
  'h',
  'i',
  'j',
  'k',
  'l',
  'm',
  'n',
  'o',
  'p',
  'q',
  'r',
  's',
  't',
  'u',
  'v',
  'w',
  'x',
  'y',
  'z',
  '_',
]

export class KeywordCompletionProvider implements vscode.CompletionItemProvider {
  provideCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position,
    token: vscode.CancellationToken,
    context: vscode.CompletionContext,
  ): vscode.ProviderResult<vscode.CompletionItem[] | vscode.CompletionList> {
    const line = document.lineAt(position).text
    const prefix = line.slice(0, position.character)
    const match = prefix.match(/@(\w*)$/)

    if (match) {
      const [, userTyped] = match

      const startPos = position.translate(0, -userTyped.length - 1) // -1 to account for "@"
      const endPos = position.translate(0, line.length - position.character)
      const replaceRange = new vscode.Range(startPos, endPos)

      const completion = keywords
        .filter((keyword) => keyword.startsWith(`@${userTyped}`))
        .map((keyword) => {
          const item = new vscode.CompletionItem(keyword, vscode.CompletionItemKind.Keyword)
          // item.insertText = keyword.slice(1);
          item.range = replaceRange
          item.filterText = '@'
          return item
        })
      console.log(completion)
      return completion
    }
  }
}
