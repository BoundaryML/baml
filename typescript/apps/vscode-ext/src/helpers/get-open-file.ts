import * as vscode from 'vscode';

export const getCurrentOpenedFile = () => {
  // This should be called from the extension host, not the language server
  // as the language server doesn't have direct access to VSCode's window API
  const activeEditor = vscode.window.activeTextEditor;
  if (activeEditor?.document?.uri) {
    return activeEditor.document.uri.toString();
  }

  const visibleEditors = vscode.window.visibleTextEditors;
  if (visibleEditors.length > 0) {
    return visibleEditors[0]?.document.uri?.toString() ?? '';
  }
};
