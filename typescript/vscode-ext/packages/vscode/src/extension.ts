/* eslint-disable @typescript-eslint/no-var-requires */
/* eslint-disable @typescript-eslint/no-unsafe-assignment */
import * as vscode from 'vscode'
import { ExtensionContext } from 'vscode'

import plugins from './plugins'

// const { exec } = require('child_process')

export function activate(context: vscode.ExtensionContext) {
  console.log('activating bamll')
  const config = vscode.workspace.getConfiguration('baml')
  const glooPath = config.get<string>('path', 'baml')

  // let disposable = vscode.workspace.onDidSaveTextDocument((document) => {
  //   if (document.fileName.endsWith(".baml")) {
  //     runBuildScript(document, glooPath);
  //   }
  // });
  plugins.map(async (plugin) => {
    const enabled = await plugin.enabled()
    if (enabled) {
      console.log(`Activating ${plugin.name}`)
      if (plugin.activate) {
        await plugin.activate(context)
      }
    } else {
      console.log(`${plugin.name} is Disabled`)
    }
  })

  // context.subscriptions.push(disposable);
}

const LANG_NAME = 'Baml'

const diagnosticsCollection = vscode.languages.createDiagnosticCollection('baml')

const outputChannel = vscode.window.createOutputChannel('baml')

function runBuildScript(document: vscode.TextDocument, glooPath: string): void {
  const buildCommand = `${glooPath} build`

  const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri)

  if (!workspaceFolder) {
    return
  }
  const options = {
    cwd: workspaceFolder.uri.fsPath,
  }

  // exec(buildCommand, options, (error: Error | null, stdout: string, stderr: string) => {
  //   if (stdout) {
  //     outputChannel.appendLine(stdout)
  //   }
  //   if (error) {
  //     vscode.window.showErrorMessage(`Error running the build script: ${error}`, 'Show Details').then((selection) => {
  //       if (selection === 'Show Details') {
  //         outputChannel.appendLine(`Error running the build script: ${error}`)
  //         outputChannel.show(true)
  //       }
  //     })
  //     return
  //   }
  //   if (stderr) {
  //     // Parse the error message to extract line number
  //     const lineMatch = stderr.match(/Line (\d+),/)
  //     if (lineMatch) {
  //       const lineNumber = parseInt(lineMatch[1], 10) - 1

  //       const range = new vscode.Range(lineNumber, 0, lineNumber, Number.MAX_VALUE)
  //       const diagnostic = new vscode.Diagnostic(range, stderr, vscode.DiagnosticSeverity.Error)

  //       diagnosticsCollection.set(document.uri, [diagnostic])
  //     } else {
  //       vscode.window.showErrorMessage(`Build error: ${stderr}`, 'Show Details').then((selection) => {
  //         if (selection === 'Show Details') {
  //           const outputChannel = vscode.window.createOutputChannel('baml')
  //           outputChannel.appendLine(`Error running the build script: ${stderr}`)
  //           outputChannel.show(true)
  //         }
  //       })
  //     }
  //     return
  //   }

  //   // Clear any diagnostics if the build was successful
  //   diagnosticsCollection.clear()

  //   const infoMessage = vscode.window.showInformationMessage('Baml build was successful')

  //   setTimeout(() => {
  //     infoMessage.then
  //   }, 5000)
  // })
}

export function deactivate(): void {
  plugins.forEach((plugin) => {
    if (plugin.deactivate) {
      void plugin.deactivate()
    }
  })
}
