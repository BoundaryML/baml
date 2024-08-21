import * as path from 'path'

import type { ParserDatabase, TestRequest } from '@baml/common'
import fetch from 'node-fetch'
import semver from 'semver'
import { type ExtensionContext, OutputChannel, Uri, ViewColumn, commands, window, workspace } from 'vscode'
import * as vscode from 'vscode'
import type { LanguageClientOptions } from 'vscode-languageclient'
import { type LanguageClient, type ServerOptions, TransportKind } from 'vscode-languageclient/node'
import { z } from 'zod'
import pythonToBamlCodeLens from '../../LanguageToBamlCodeLensProvider'
import { WebPanelView } from '../../panels/WebPanelView'
import TelemetryReporter from '../../telemetryReporter'
import { checkForMinimalColorTheme, createLanguageServer, isDebugOrTestSession, restartClient } from '../../util'
import type { BamlVSCodePlugin } from '../types'
import { URI } from 'vscode-uri'
import StatusBarPanel from '../../panels/StatusBarPanel'

const packageJson = require('../../../package.json') // eslint-disable-line

const BamlConfig = z.optional(
  z.object({
    path: z.string().optional(),
    trace: z.optional(
      z.object({
        server: z.string(),
      }),
    ),
  }),
)
type BamlConfig = z.infer<typeof BamlConfig>
let config: BamlConfig | null = null
let client: LanguageClient
let serverModule: string
let telemetry: TelemetryReporter
const intervalTimers: NodeJS.Timeout[] = []

export const bamlConfig: { config: BamlConfig | null; cliVersion: string | null } = {
  config: null,
  cliVersion: null,
}

const isDebugMode = () => process.env.VSCODE_DEBUG_MODE === 'true'
const isE2ETestOnPullRequest = () => process.env.PRISMA_USE_LOCAL_LS === 'true'

export const generateTestRequest = async (test_request: TestRequest): Promise<string | undefined> => {
  return await client.sendRequest('generatePythonTests', test_request)
}

export const requestDiagnostics = async () => {
  await client?.sendRequest('requestDiagnostics')
}

export const requestBamlCLIVersion = async () => {
  try {
    const version = await client.sendRequest('bamlCliVersion')
    console.log('Got BAML CLI version', version)
    bamlConfig.cliVersion = version as string
  } catch (e) {
    console.error('Failed to get BAML CLI version', e)
  }
}

export const getBAMLFunctions = async (): Promise<
  {
    name: string
    span: { file_path: string; start: number; end: number }
  }[]
> => {
  return await client.sendRequest('getBAMLFunctions')
}

const LatestVersions = z.object({
  cli: z.object({
    current_version: z.string(),
    latest_version: z.string().nullable(),
    recommended_update: z.string().nullable(),
  }),
  generators: z.array(
    z.object({
      name: z.string(),
      current_version: z.string(),
      latest_version: z.string().nullable(),
      recommended_update: z.string().nullable(),
      language: z.string(),
    }),
  ),
  vscode: z.object({
    latest_version: z.string().nullable(),
  }),
})
type LatestVersions = z.infer<typeof LatestVersions>

const checkForUpdates = async ({ showIfNoUpdates }: { showIfNoUpdates: boolean }) => {
  try {
    if (telemetry) {
      telemetry.sendTelemetryEvent({
        event: 'baml.checkForUpdates',
        properties: {
          // is_typescript: latestVersions.generators.find((g) => g.language === 'typescript'),
          // is_python: latestVersions.generators.find((g) => g.language === 'python'),
          // baml_check: latestVersions,
          // updateAvailable: !!update,
          // vscodeUpdateAvailable: shouldUpdateVscode,
        },
      })
    }
  } catch (e) {
    console.error('Failed to check for updates', e)
  }
}

interface BAMLMessage {
  type: 'warn' | 'info' | 'error'
  message: string
  durationMs?: number
}

const sleep = (time: number) => {
  return new Promise((resolve) => {
    setTimeout(() => {
      resolve(true)
    }, time)
  })
}

const getConfig = async () => {
  try {
    console.log('getting config')
    const configResponse = await workspace.getConfiguration('baml')
    console.log('configResponse ' + JSON.stringify(configResponse, null, 2))
    config = BamlConfig.parse(configResponse)
  } catch (e: any) {
    if (e instanceof Error) {
      console.log('Error getting config' + e.message + ' ' + e.stack)
    } else {
      console.log('Error getting config' + e)
    }
  }
}

const activateClient = (
  context: ExtensionContext,
  serverOptions: ServerOptions,
  clientOptions: LanguageClientOptions,
) => {
  getConfig()

  // Create the language client
  client = createLanguageServer(serverOptions, clientOptions)

  client.onReady().then(() => {
    client.onNotification('baml/showLanguageServerOutput', () => {
      // need to append line for the show to work for some reason.
      // dont delete this.
      client.outputChannel.appendLine('\n')
      client.outputChannel.show()
    })
    client.onNotification('baml/message', (message: BAMLMessage) => {
      client.outputChannel.appendLine('baml/message' + JSON.stringify(message, null, 2))
      let msg: Thenable<any>
      switch (message.type) {
        case 'warn': {
          msg = window.showWarningMessage(message.message)
          break
        }
        case 'info': {
          window.withProgress(
            {
              location: vscode.ProgressLocation.Notification,
              cancellable: false,
            },
            async (progress, token) => {
              let customCancellationToken: vscode.CancellationTokenSource | null = null
              return new Promise(async (resolve) => {
                customCancellationToken = new vscode.CancellationTokenSource()

                customCancellationToken.token.onCancellationRequested(() => {
                  customCancellationToken?.dispose()
                  customCancellationToken = null

                  // vscode.window.showInformationMessage('Cancelled the progress')
                  resolve(null)
                  return
                })

                const totalMs = message.durationMs || 1500 // Total duration in milliseconds (2 seconds)
                const updateCount = 50 // Number of updates
                const intervalMs = totalMs / updateCount // Interval between updates

                for (let i = 0; i < updateCount; i++) {
                  const prog = ((i + 1) / updateCount) * 100
                  progress.report({ increment: prog, message: message.message })
                  await sleep(intervalMs)
                }
                resolve(null)
              })
            },
          )
          break
        }
        case 'error': {
          window.showErrorMessage(message.message)
          break
        }
        default: {
          throw new Error('Invalid message type')
        }
      }
    })

    client.onRequest('runtime_diagnostics', ({ errors, warnings }: { errors: number; warnings: number }) => {
      try {
        if (errors > 0) {
          StatusBarPanel.instance.setStatus({ status: 'fail', count: errors })
        } else if (warnings > 0) {
          StatusBarPanel.instance.setStatus({ status: 'warn', count: warnings })
        } else {
          StatusBarPanel.instance.setStatus('pass')
        }
      } catch (e) {
        console.error('Error updating status bar', e)
      }
    })

    client.onRequest('executeCommand', async (command: string) => {
      try {
        console.log('Executing command', command)
        await vscode.commands.executeCommand(command)
      } catch (e) {
        console.error('Error executing command', e)
      }
    })

    client.onRequest('runtime_updated', (params: { root_path: string; files: Record<string, string> }) => {
      WebPanelView.currentPanel?.postMessage('add_project', {
        ...params,
        root_path: URI.file(params.root_path).toString(),
      })
    })

    // this will fail otherwise in dev mode if the config where the baml path is hasnt been picked up yet. TODO: pass the config to the server to avoid this.
    // Immediately check for updates on extension activation
    void checkForUpdates({ showIfNoUpdates: false })
    // And check again once every hour
    intervalTimers.push(
      setInterval(
        async () => {
          console.log(`checking for updates ${new Date().toString()}`)
          await checkForUpdates({ showIfNoUpdates: false })
        },
        60 * 60 * 1000 /* 1h in milliseconds: min/hr * secs/min * ms/sec */,
      ),
    )
  })

  const disposable = client.start()

  // Start the client. This will also launch the server
  context.subscriptions.push(disposable)
}

const onFileChange = (filepath: string) => {
  console.debug(`File ${filepath} has changed, restarting TS Server.`)
  void commands.executeCommand('typescript.restartTsServer')
}

const plugin: BamlVSCodePlugin = {
  name: 'baml-language-server',
  enabled: () => true,
  activate: async (context, _outputChannel) => {
    const isDebugOrTest = isDebugOrTestSession()

    // setGenerateWatcher(!!workspace.getConfiguration('baml').get('fileWatcher'))

    // eslint-disable-next-line @typescript-eslint/no-unsafe-member-access
    // if (packageJson.name === 'prisma-insider-pr-build') {
    //   console.log('Using local Language Server for prisma-insider-pr-build');
    //   serverModule = context.asAbsolutePath(path.join('./language-server/dist/src/bin'));
    // } else if (isDebugMode() || isE2ETestOnPullRequest()) {
    //   // use Language Server from folder for debugging
    //   console.log('Using local Language Server from filesystem');
    //   serverModule = context.asAbsolutePath(path.join('../../packages/language-server/dist/src/bin'));
    // } else {
    //   console.log('Using published Language Server (npm)');
    //   // use published npm package for production
    //   serverModule = require.resolve('@prisma/language-server/dist/src/bin');
    // }
    console.log('debugmode', isDebugMode())
    // serverModule = context.asAbsolutePath(path.join('../../packages/language-server/dist/src/bin'))

    serverModule = context.asAbsolutePath(path.join('language-server', 'out', 'bin'))

    console.log(`serverModules: ${serverModule}`)

    // The debug options for the server
    // --inspect=6009: runs the server in Node's Inspector mode so VS Code can attach to the server for debugging
    const debugOptions = {
      execArgv: ['--nolazy', '--inspect=6009'],
      env: { DEBUG: true },
    }

    // If the extension is launched in debug mode then the debug server options are used
    // Otherwise the run options are used
    const serverOptions: ServerOptions = {
      run: { module: serverModule, transport: TransportKind.ipc },
      debug: {
        module: serverModule,
        transport: TransportKind.ipc,
        options: debugOptions,
      },
    }

    // Options to control the language client
    const clientOptions: LanguageClientOptions = {
      // Register the server for baml docs and python
      documentSelector: [
        { scheme: 'file', language: 'baml' },
        {
          language: 'json',
          pattern: '**/baml_src/**',
        },
      ],
      synchronize: {
        fileEvents: workspace.createFileSystemWatcher('**/baml_src/**/*.{baml,json}'),
      },
    }

    context.subscriptions.push(
      commands.registerCommand('baml.restartLanguageServer', async () => {
        client = await restartClient(context, client, serverOptions, clientOptions)
        window.showInformationMessage('Baml language server restarted.') // eslint-disable-line @typescript-eslint/no-floating-promises
      }),

      commands.registerCommand('baml.checkForUpdates', async () => {
        checkForUpdates({ showIfNoUpdates: true }).catch((e) => {
          console.error('Failed to check for updates', e)
        })
      }),

      commands.registerCommand(
        'baml.selectTestCase',
        async (test_request: {
          functionName?: string
          testCaseName?: string
        }) => {
          const { functionName, testCaseName } = test_request
          if (!functionName || !testCaseName) {
            return
          }

          console.log('selectTestCase', functionName, testCaseName)
          await client.sendRequest('selectTestCase', { functionName, testCaseName })
        },
      ),

      commands.registerCommand(
        'baml.jumpToDefinition',
        async (args: { file_path: string; start: number; end: number }) => {
          if (!args.file_path) {
            vscode.window.showErrorMessage('File path is missing.')
            return
          }

          try {
            const uri = vscode.Uri.file(args.file_path)
            const doc = await vscode.workspace.openTextDocument(uri)

            const start = doc.positionAt(args.start)
            const end = doc.positionAt(args.end)
            const range = new vscode.Range(start, end)

            await vscode.window.showTextDocument(doc, { selection: range, viewColumn: vscode.ViewColumn.Beside })
          } catch (error) {
            vscode.window.showErrorMessage(`Error navigating to function definition: ${error}`)
          }
        },
      ),
    )

    activateClient(context, serverOptions, clientOptions)

    if (!isDebugOrTest) {
      // eslint-disable-next-line
      const extensionId = 'Gloo.' + packageJson.name
      // eslint-disable-next-line
      const extensionVersion: string = packageJson.version

      telemetry = new TelemetryReporter(extensionId, extensionVersion)

      context.subscriptions.push(telemetry)
      await telemetry.initialize()

      if (extensionId === 'Boundary.baml-insider') {
        // checkForOtherExtension()
      }
    }

    checkForMinimalColorTheme()
  },
  deactivate: async () => {
    if (!client) {
      return undefined
    }

    if (!isDebugOrTestSession()) {
      telemetry.dispose() // eslint-disable-line @typescript-eslint/no-floating-promises
    }

    while (intervalTimers.length > 0) {
      clearInterval(intervalTimers.pop())
    }

    return client.stop()
  },
}

export { telemetry }
export default plugin
