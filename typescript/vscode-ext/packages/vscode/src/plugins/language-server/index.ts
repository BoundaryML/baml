import * as path from 'path'

import { commands, ExtensionContext, OutputChannel, ViewColumn, Uri, window, workspace } from 'vscode'
import { LanguageClientOptions } from 'vscode-languageclient'
import { LanguageClient, ServerOptions, TransportKind } from 'vscode-languageclient/node'
import TelemetryReporter from '../../telemetryReporter'
import { checkForMinimalColorTheme, createLanguageServer, isDebugOrTestSession, restartClient } from '../../util'
import { BamlVSCodePlugin } from '../types'
import * as vscode from 'vscode'
import { WebPanelView } from '../../panels/WebPanelView'
import { ParserDatabase, TestRequest } from '@baml/common'
import glooLens from '../../GlooCodeLensProvider'
import fetch from 'node-fetch'
import semver from 'semver'
import { z } from 'zod'

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
let intervalTimers: NodeJS.Timer[] = []

const isDebugMode = () => process.env.VSCODE_DEBUG_MODE === 'true'
const isE2ETestOnPullRequest = () => process.env.PRISMA_USE_LOCAL_LS === 'true'

export const BamlDB = new Map<string, any>()

export const generateTestRequest = async (test_request: TestRequest): Promise<string | undefined> => {
  return await client.sendRequest('generatePythonTests', test_request)
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

const bamlCliPath = () => config?.path || 'baml'

const buildUpdateMessage = (latestVersions: LatestVersions): { message: string; command: string } | null => {
  const shouldUpdateCli = !!latestVersions.cli.recommended_update
  const shouldUpdateGenerators = latestVersions.generators.filter((g) => g.recommended_update).length > 0

  if (shouldUpdateCli && shouldUpdateGenerators) {
    return {
      message: 'Please update BAML and its client libraries',
      command: `${bamlCliPath()} update && ${bamlCliPath()} update-client`,
    }
  }

  if (shouldUpdateCli) {
    return {
      message: 'Please update BAML',
      command: `${bamlCliPath()} update`,
    }
  }

  if (shouldUpdateGenerators) {
    return {
      message: 'Please update the BAML client libraries',
      command: `${bamlCliPath()} update-client`,
    }
  }

  return null
}

const checkForUpdates = async ({ showIfNoUpdates }: { showIfNoUpdates: boolean }) => {
  try {
    const res = await client.sendRequest<string | undefined>('cliCheckForUpdates')

    if (!res) {
      throw new Error('Failed to get latest updates')
    }

    const latestVersions = LatestVersions.parse(JSON.parse(res))
    const update = buildUpdateMessage(latestVersions)
    const shouldUpdateVscode = semver.lt(packageJson.version, latestVersions.vscode.latest_version || '0.0.0')

    if (update) {
      const updateNowAction = 'Update now'
      const detailsAction = 'Details'
      vscode.window
        .showInformationMessage(
          update.message,
          {
            title: updateNowAction,
          },
          {
            title: detailsAction,
          },
        )
        .then((selection) => {
          if (selection?.title === updateNowAction) {
            // Open a new terminal
            vscode.commands.executeCommand('workbench.action.terminal.new').then(() => {
              // Run the update command
              vscode.commands.executeCommand('workbench.action.terminal.sendSequence', {
                text: `${update.command}\n`,
              })
            })
          }
          if (selection?.title === detailsAction) {
            // Open a new terminal
            vscode.commands.executeCommand('workbench.action.terminal.new').then(() => {
              // Run the update command
              vscode.commands.executeCommand('workbench.action.terminal.sendSequence', {
                text: `${bamlCliPath()} version --check\n`,
              })
            })
          }
        })
    } else {
      if (showIfNoUpdates) {
        vscode.window.showInformationMessage(`BAML is up to date!`)
      } else {
        console.info(`BAML is up to date! ${JSON.stringify(latestVersions, null, 2)}`)
      }
    }

    if (shouldUpdateVscode) {
      const updateNowAction = 'Open Extensions View'
      vscode.window
        .showInformationMessage('Please update the BAML VSCode extension', {
          title: updateNowAction,
        })
        .then((selection) => {
          if (selection?.title === updateNowAction) {
            vscode.commands.executeCommand('workbench.view.extensions')
          }
        })
    }

    telemetry.sendTelemetryEvent({
      event: 'baml.checkForUpdates',
      properties: {
        is_typescript: latestVersions.generators.find((g) => g.language === 'typescript'),
        is_python: latestVersions.generators.find((g) => g.language === 'python'),
        baml_check: latestVersions,
        updateAvailable: !!update,
        vscodeUpdateAvailable: shouldUpdateVscode,
      },

    })
  } catch (e) {
    console.error('Failed to check for updates', e)
  }
}

interface BAMLMessage {
  type: 'warn' | 'info' | 'error'
  message: string
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
      client.outputChannel.appendLine('baml/showLanguageServerOutput')
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

                  vscode.window.showInformationMessage('Cancelled the progress')
                  resolve(null)
                  return
                })

                const sleepTimeMs = 1000
                const totalSecs = 10
                const iterations = (totalSecs * 1000) / sleepTimeMs
                for (let i = 0; i < iterations; i++) {
                  const prog = (i / iterations) * 100
                  // Increment is summed up with the previous value
                  progress.report({ increment: prog, message: `BAML Client generated!` })
                  await sleep(100)
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

    client.onRequest('set_database', ({ rootPath, db }: { rootPath: string; db: ParserDatabase }) => {
      try {
        BamlDB.set(rootPath, db)
        glooLens.setDB(rootPath, db)
        console.log('set_database')
        WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
      } catch (e) {
        console.log('Error setting database', e)
      }
    })
    client.onRequest('rm_database', (root_path) => {
      // TODO: Handle errors better. But for now the playground shouldn't break.
      // BamlDB.delete(root_path)
      // WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
    })

    // this will fail otherwise in dev mode if the config where the baml path is hasnt been picked up yet. TODO: pass the config to the server to avoid this.
    // Immediately check for updates on extension activation
    void checkForUpdates({ showIfNoUpdates: false })
    // And check again once every hour
    intervalTimers.push(
      setInterval(async () => {
        console.log(`checking for updates ${new Date()}`)
        await checkForUpdates({ showIfNoUpdates: false })
      }, 60 * 60 * 1000 /* 1h in milliseconds: min/hr * secs/min * ms/sec */),
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
      // Register the server for baml docs
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

      commands.registerCommand('baml.selectTestCase', async (test_request: {
        functionName?: string
        testCaseName?: string
      }) => {
        const { functionName, testCaseName } = test_request
        if (!functionName || !testCaseName) {
          return
        }

        console.log('selectTestCase', functionName, testCaseName)
        await client.sendRequest('selectTestCase', { functionName, testCaseName });
      }),

      commands.registerCommand('baml.jumpToDefinition', async (args: { sourceFile?: string; name?: string }) => {
        let { sourceFile, name } = args
        if (!sourceFile || !name) {
          return
        }

        let response = await client.sendRequest('getDefinition', { sourceFile, name })
        if (response) {
          let { targetUri, targetRange, targetSelectionRange } = response as {
            targetUri: string
            targetRange: {
              start: { line: number; column: number }
              end: { line: number; column: number }
            }
            targetSelectionRange: {
              start: { line: number; column: number }
              end: { line: number; column: number }
            }
          }
          let uri = Uri.parse(targetUri)
          let doc = await workspace.openTextDocument(uri)
          // go to line
          let selection = new vscode.Selection(targetSelectionRange.start.line, 0, targetSelectionRange.end.line, 0)
          await window.showTextDocument(doc, { selection, viewColumn: ViewColumn.Beside })
        }
      }),
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

      if (extensionId === 'Gloo.baml-insider') {
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
