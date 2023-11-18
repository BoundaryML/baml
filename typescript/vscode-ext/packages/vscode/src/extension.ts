/* eslint-disable @typescript-eslint/no-var-requires */
/* eslint-disable @typescript-eslint/no-unsafe-assignment */
import * as vscode from 'vscode'

import plugins from './plugins'
import { WebPanelView } from './panels/WebPanelView'
import { BamlDB } from './plugins/language-server'

const outputChannel = vscode.window.createOutputChannel('baml')
const diagnosticsCollection = vscode.languages.createDiagnosticCollection('baml')
const LANG_NAME = 'Baml'

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('baml')

  const bamlPlygroundCommand = vscode.commands.registerCommand('baml.openBamlPanel', () => {
    const config = vscode.workspace.getConfiguration()
    config.update('baml.bamlPanelOpen', true, vscode.ConfigurationTarget.Global)

    WebPanelView.render(context.extensionUri)
    WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
  })

  context.subscriptions.push(bamlPlygroundCommand)

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
}

export function deactivate(): void {
  plugins.forEach((plugin) => {
    if (plugin.deactivate) {
      void plugin.deactivate()
    }
  })
}
