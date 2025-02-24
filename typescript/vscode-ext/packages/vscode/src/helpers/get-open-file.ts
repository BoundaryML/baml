import { URI } from 'vscode-uri'
import * as vscode from 'vscode'

export const getCurrentOpenedFile = () => {
  // This should be called from the extension host, not the language server
  // as the language server doesn't have direct access to VSCode's window API
  return vscode.window.activeTextEditor?.document?.uri?.toString()
}
