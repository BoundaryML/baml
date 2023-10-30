/* eslint-disable @typescript-eslint/no-var-requires */
/* eslint-disable @typescript-eslint/no-unsafe-assignment */
import * as vscode from 'vscode'
import { ExtensionContext } from 'vscode'

import plugins from './plugins'

// const { exec } = require('child_process')
const outputChannel = vscode.window.createOutputChannel('baml')

export function activate(context: vscode.ExtensionContext) {
  console.log('activating bamll')
  const config = vscode.workspace.getConfiguration('baml')
  const glooPath = config.get<string>('path', 'baml')


  plugins.map(async (plugin) => {
    const enabled = await plugin.enabled()
    if (enabled) {
      console.log(`Activating ${plugin.name}`)
      if (plugin.activate) {
        await plugin.activate(context, outputChannel)
      }
    } else {
      console.log(`${plugin.name} is Disabled`)
    }
  })

  // context.subscriptions.push(disposable);
}

const LANG_NAME = 'Baml'

const diagnosticsCollection = vscode.languages.createDiagnosticCollection('baml')


export function deactivate(): void {
  plugins.forEach((plugin) => {
    if (plugin.deactivate) {
      void plugin.deactivate()
    }
  })
}
