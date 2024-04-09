/* eslint-disable @typescript-eslint/no-var-requires */
/* eslint-disable @typescript-eslint/no-unsafe-assignment */
import * as vscode from 'vscode'

import plugins from './plugins'
import { WebPanelView } from './panels/WebPanelView'
import { BamlDB } from './plugins/language-server'
import testExecutor from './panels/execute_test'
import glooLens from './GlooCodeLensProvider'
import { telemetry } from './plugins/language-server'

const outputChannel = vscode.window.createOutputChannel('baml')
const diagnosticsCollection = vscode.languages.createDiagnosticCollection('baml')
const LANG_NAME = 'Baml'

export function activate(context: vscode.ExtensionContext) {
  console.log("BAML extension activating")

  vscode.workspace.getConfiguration('baml')

  const bamlPlaygroundCommand = vscode.commands.registerCommand(
    'baml.openBamlPanel',
    (args?: { projectId?: string; functionName?: string; implName?: string; showTests?: boolean }) => {
      const projectId = args?.projectId
      const initialFunctionName = args?.functionName
      const initialImplName = args?.implName
      const showTests = args?.showTests
      const config = vscode.workspace.getConfiguration()
      config.update('baml.bamlPanelOpen', true, vscode.ConfigurationTarget.Global)
      WebPanelView.render(context.extensionUri)
      telemetry.sendTelemetryEvent({
        event: 'baml.openBamlPanel',
        properties: {},
      })

      WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
      // send another request for reliability on slower machines
      // A more resilient way is to get a msg for it to finish loading but this is good enough for now
      setTimeout(() => {
        WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
      }, 2000);
      WebPanelView.currentPanel?.postMessage('setSelectedResource', {
        projectId: projectId,
        functionName: initialFunctionName,
        implName: initialImplName,
        testCaseName: undefined,
        showTests: showTests,
      })
    },
  )

  context.subscriptions.push(bamlPlaygroundCommand)
  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider({ scheme: 'file', language: 'python' }, glooLens),
  )

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

  testExecutor.start()

  if (process.env.VSCODE_DEBUG_MODE === "true") {
    console.log(`vscode env: ${JSON.stringify(process.env, null, 2)}`)
    vscode.commands.executeCommand('baml.openBamlPanel')
  }
}

export function deactivate(): void {
  console.log("BAML extension deactivating")
  testExecutor.close()
  plugins.forEach((plugin) => {
    if (plugin.deactivate) {
      void plugin.deactivate()
    }
  })
}
