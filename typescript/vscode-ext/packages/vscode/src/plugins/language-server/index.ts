import * as fs from 'fs'
import * as os from 'os'
import * as path from 'path'

import type { ParserDatabase, TestRequest } from '@baml/common'
import fetch from 'node-fetch'
import semver from 'semver'
import { type ExtensionContext, OutputChannel, Uri, ViewColumn, commands, window, workspace } from 'vscode'
import * as vscode from 'vscode'
import type { LanguageClientOptions } from 'vscode-languageclient'
import {
  type LanguageClient,
  RevealOutputChannelOn,
  type ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node'
import { z } from 'zod'
import pythonToBamlCodeLens from '../../LanguageToBamlCodeLensProvider'
import { WebviewPanelHost } from '../../panels/WebviewPanelHost'
import TelemetryReporter from '../../telemetryReporter'
import { checkForMinimalColorTheme, createLanguageServer, isDebugOrTestSession, restartClient } from '../../util'
import type { BamlVSCodePlugin } from '../types'
import { URI } from 'vscode-uri'
import StatusBarPanel from '../../panels/StatusBarPanel'
import { getCurrentOpenedFile } from '../../helpers/get-open-file'
import { bamlConfig, getConfig } from './bamlConfig'

export { bamlConfig }
const packageJson = require('../../../../package.json') // eslint-disable-line
let clientReady = false

let client: LanguageClient
let serverModule: string
let telemetry: TelemetryReporter
const intervalTimers: NodeJS.Timeout[] = []

const isDebugMode = () => process.env.VSCODE_DEBUG_MODE === 'true'
const isE2ETestOnPullRequest = () => process.env.PRISMA_USE_LOCAL_LS === 'true'

export const requestDiagnostics = async () => {
  const currentFile = getCurrentOpenedFile()
  if (!currentFile) {
    console.warn('no current baml file')
    return
  }
  // if not a baml file return
  if (!currentFile.endsWith('.baml')) {
    return
  }
  if (!clientReady) {
    console.warn('client not ready')
    return
  }
  await client?.sendRequest('requestDiagnostics', { projectId: currentFile })
}

export const requestBamlCLIVersion = async () => {
  try {
    console.log('requesting baml cli version')
    const version = await client?.sendRequest('bamlCliVersion')
    if (!version) {
      return
    }
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

const checkForUpdates = ({ showIfNoUpdates }: { showIfNoUpdates: boolean }) => {
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

const activateClient = (
  context: ExtensionContext,
  serverOptions: ServerOptions,
  clientOptions: LanguageClientOptions,
) => {
  getConfig()
  console.log('Starting language server with options', JSON.stringify(serverOptions, null, 2))

  // Create the language client
  client = createLanguageServer(serverOptions, clientOptions)
  console.log('client created')
  client
    .onReady()
    .then(() => {
      console.log('client ready')
      clientReady = true
      client.createDefaultErrorHandler(2)
      requestDiagnostics()
      client.onNotification('baml/showLanguageServerOutput', () => {
        // need to append line for the show to work for some reason.
        // dont delete this.
        client.outputChannel.appendLine('\n')
        client.outputChannel.show(true)
      })
      client.onNotification('baml/message', (message: BAMLMessage) => {
        console.log('baml/message', message)
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
                const rest = new Promise<null>((resolve) => {
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
                  ;(async () => {
                    for (let i = 0; i < updateCount; i++) {
                      const prog = ((i + 1) / updateCount) * 100
                      progress.report({ increment: prog, message: message.message })
                      await sleep(intervalMs)
                    }
                    resolve(null)
                  })()
                })

                return rest
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

      client.onNotification('runtime_diagnostics', (params: { errors: number; warnings: number }) => {
        console.log('runtime_diagnostics', params)
        try {
          if (params.errors > 0) {
            StatusBarPanel.instance.setStatus({ status: 'fail', count: params.errors })
          } else if (params.warnings > 0) {
            StatusBarPanel.instance.setStatus({ status: 'warn', count: params.warnings })
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

      client.onRequest('baml_settings_updated', (config: typeof bamlConfig) => {
        console.log('baml_settings_updated', config)
        bamlConfig.config = config.config
        bamlConfig.cliVersion = config.cliVersion
        WebviewPanelHost.currentPanel?.postMessage('baml_settings_updated', bamlConfig)
      })

      // Handler for both notifications and requests of type "runtime_updated".
      const handleRuntimeUpdated = (params: { root_path: string; files: Record<string, string> }) => {
        // console.log('*** HANDLE RUNTIME UPDATED ***' + JSON.stringify(params, null, 2))
        // Only send message if current file is part of this root path
        const activeEditor =
          vscode.window.activeTextEditor ||
          (vscode.window.visibleTextEditors.length > 0 ? vscode.window.visibleTextEditors[0] : null)
        if (activeEditor) {
          const currentFilePath = URI.parse(activeEditor.document.uri.toString()).fsPath
          const rootPathUri = URI.file(params.root_path).fsPath
          if (currentFilePath.startsWith(rootPathUri)) {
            console.log('sending add_project message')
            WebviewPanelHost.currentPanel?.postMessage('add_project', {
              ...params,
              root_path: URI.file(params.root_path).toString(),
            })
          } else {
            console.log('root path doesnt match current file', currentFilePath, rootPathUri)
          }
        } else {
          console.log('no active editor')
        }
      }

      // The Node-based Language Server sends REQUESTS of type "runtime_updated".
      client.onRequest('runtime_updated', (params: { root_path: string; files: Record<string, string> }) => {
        console.log('REQUEST: runtime_updated')
        handleRuntimeUpdated(params)
      })

      // The Web-based Language Server sends NOTIFICATIONS of type "runtime_updated".
      client.onNotification('runtime_updated', (params: { root_path: string; files: Record<string, string> }) => {
        console.log('NOTIF: runtime_updated')
        handleRuntimeUpdated(params)
      })

      client.onRequest('baml_settings_updated', (config: typeof bamlConfig) => {
        console.log('baml_settings_updated', config)
        bamlConfig.config = config.config
        bamlConfig.cliVersion = config.cliVersion
        WebviewPanelHost.currentPanel?.postMessage('baml_settings_updated', bamlConfig)
      })

      // this will fail otherwise in dev mode if the config where the baml path is hasnt been picked up yet. TODO: pass the config to the server to avoid this.
      // Immediately check for updates on extension activation
      void checkForUpdates({ showIfNoUpdates: false })
      // And check again once every hour
      intervalTimers.push(
        setInterval(
          () => {
            console.log(`checking for updates ${new Date().toString()}`)
            checkForUpdates({ showIfNoUpdates: false })
          },
          6 * 60 * 60 * 1000 /* 6h in milliseconds: min/hr * secs/min * ms/sec */,
        ),
      )
    })
    .catch((err) => {
      console.error('Error activating client', err)
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

    // The debug options for the server
    // --inspect=6009: runs the server in Node's Inspector mode so VS Code can attach to the server for debugging
    const debugOptions = {
      execArgv: ['--nolazy', '--inspect=6009'],
      env: {
        DEBUG: true,
        // This will show stack traces in VSCODE notifications in debug mode.
        RUST_BACKTRACE: 'full',
        ...process.env,
      },
    }

    // If the extension is launched in debug mode then the debug server options are used
    // Otherwise the run options are used
    // const serverOptions: ServerOptions = {
    //   run: { module: serverModule, transport: TransportKind.ipc },
    //   debug: {
    //     module: serverModule,
    //     transport: TransportKind.ipc,
    //     options: debugOptions,
    //   },
    // }

    let serverExecutableName = 'baml-cli'
    let targetTriple = ''
    const platform = os.platform()
    const arch = os.arch()

    switch (platform) {
      case 'win32':
        serverExecutableName = `${serverExecutableName}.exe`
        if (arch === 'x64') {
          targetTriple = 'x86_64-pc-windows-msvc'
        } else if (arch === 'arm64') {
          targetTriple = 'aarch64-pc-windows-msvc'
        }
        break
      case 'darwin':
        if (arch === 'x64') {
          targetTriple = 'x86_64-apple-darwin'
        } else if (arch === 'arm64') {
          targetTriple = 'aarch64-apple-darwin'
        }
        break
      case 'linux':
        // Defaulting to gnu. Musl detection is complex in VSCode extensions.
        // Users on musl systems might need a configuration option if this fails.
        if (arch === 'x64') {
          targetTriple = 'x86_64-unknown-linux-gnu'
        } else if (arch === 'arm64') {
          targetTriple = 'aarch64-unknown-linux-gnu'
        }
        break
      // Add other platforms/arches as needed
    }

    if (!targetTriple) {
      throw new Error(`Unsupported platform/architecture combination: ${platform}/${arch}`)
    }

    let serverAbsolutePath = context.asAbsolutePath(path.join('vscode', 'server', targetTriple, serverExecutableName))
    // account for windows
    const devServerPath = context.asAbsolutePath(path.join('vscode', 'server', serverExecutableName)) // Adjust dev path if necessary
    console.log('devServerPath', devServerPath)

    // If the dev server file exists, overwrite serverAbsolutePath with it for local development.
    if (fs.existsSync(devServerPath)) {
      console.log('Using dev server path:', devServerPath)
      serverAbsolutePath = devServerPath
    } else {
      // Check if the bundled server exists at the determined path
      if (!fs.existsSync(serverAbsolutePath)) {
        // Fallback or specific error handling if the primary target binary isn't found
        // For example, try the musl variant on Linux if gnu wasn't found?
        if (platform === 'linux' && targetTriple.endsWith('-gnu')) {
          const muslTargetTriple = targetTriple.replace('-gnu', '-musl')
          const muslServerPath = context.asAbsolutePath(path.join('server', muslTargetTriple, serverExecutableName))
          if (fs.existsSync(muslServerPath)) {
            console.log(`GNU variant not found for ${arch}, falling back to MUSL variant.`)
            serverAbsolutePath = muslServerPath
            targetTriple = muslTargetTriple // Update targetTriple for clarity if needed elsewhere
          } else {
            window.showErrorMessage(
              `BAML Language Server executable not found for your system (${platform}/${arch}). Tried: ${serverAbsolutePath} and ${muslServerPath}`,
            )
            throw new Error(`BAML Language Server executable not found for ${targetTriple} or ${muslTargetTriple}.`)
          }
        } else {
          window.showErrorMessage(
            `BAML Language Server executable not found for your system (${platform}/${arch}). Expected at: ${serverAbsolutePath}`,
          )
          throw new Error(`BAML Language Server executable not found for ${targetTriple}.`)
        }
      }
    }

    console.log(`Using BAML Language Server: ${serverAbsolutePath}`)

    if (platform !== 'win32' && fs.existsSync(serverAbsolutePath)) {
      try {
        fs.chmodSync(serverAbsolutePath, '755')
      } catch (err: any) {
        console.error(`Failed to chmod server executable: ${err}`)
        // Decide if this should be a fatal error
      }
    }

    const serverOptions: ServerOptions = {
      run: {
        command: serverAbsolutePath,
        args: ['lsp'],
        options: {
          env: process.env,
        },
      },
      debug: {
        command: serverAbsolutePath,

        args: ['lsp'],
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
      outputChannel: vscode.window.createOutputChannel('Baml Language Server'),
      // traceOutputChannel: vscode.window.createOutputChannel('Baml Language Server Trace'),
      revealOutputChannelOn: RevealOutputChannelOn.Never,
      // initializationOptions // TODO add settings here.
      synchronize: {
        fileEvents: workspace.createFileSystemWatcher('**/baml_src/**/*.baml'),
      },
    }

    context.subscriptions.push(
      commands.registerCommand('baml.restartLanguageServer', async () => {
        client = await restartClient(context, client, serverOptions, clientOptions)
        window.showInformationMessage('Baml language server restarted.') // eslint-disable-line @typescript-eslint/no-floating-promises
      }),

      commands.registerCommand('baml.checkForUpdates', () => {
        checkForUpdates({ showIfNoUpdates: true })
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
        async (args: {
          file_path: string
          start: number
          end: number
        }) => {
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
          } catch (error: any) {
            vscode.window.showErrorMessage(`Error navigating to function definition: ${error}`)
          }
        },
      ),

      commands.registerCommand('baml.setDefaultFormatter', async () => {
        enum AutoFormatChoice {
          Yes = 'Yes (always)',
          OnlyInWorkspace = 'Yes (in workspace)',
          No = 'No',
        }
        const selection = await vscode.window.showInformationMessage(
          'Would you like to auto-format BAML files on save?',
          AutoFormatChoice.Yes,
          AutoFormatChoice.OnlyInWorkspace,
          AutoFormatChoice.No,
        )
        if (selection === AutoFormatChoice.No) {
          return
        }

        const config = vscode.workspace.getConfiguration('editor', { languageId: 'baml' })

        const configTarget =
          selection === AutoFormatChoice.Yes ? vscode.ConfigurationTarget.Global : vscode.ConfigurationTarget.Workspace
        const overrideInLanguage = true

        for (const [key, value] of Object.entries({
          defaultFormatter: 'Boundary.baml-extension',
          formatOnSave: true,
        })) {
          await config.update(key, value, configTarget, overrideInLanguage)
        }

        switch (selection) {
          case AutoFormatChoice.Yes:
            vscode.window.showInformationMessage(
              'BAML files will now be auto-formatted on save (updated user settings).',
            )
            break
          case AutoFormatChoice.OnlyInWorkspace:
            vscode.window.showInformationMessage(
              'BAML files will now be auto-formatted on save (updated workspace settings).',
            )
            break
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
