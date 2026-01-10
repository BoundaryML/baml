import * as vscode from 'vscode';
import { WebviewPanel } from './panels/WebviewPanel';

export function activate(context: vscode.ExtensionContext) {
  console.log('BAML Playground extension activating');

  const openPlaygroundCommand = vscode.commands.registerCommand(
    'baml.openPlayground',
    () => {
      WebviewPanel.render(context.extensionUri);
    }
  );

  context.subscriptions.push(openPlaygroundCommand);

  // Auto-open in debug mode
  if (process.env.VSCODE_DEBUG_MODE === 'true') {
    vscode.commands.executeCommand('baml.openPlayground');
  }
}

export function deactivate(): void {
  console.log('BAML Playground extension deactivating');
}
