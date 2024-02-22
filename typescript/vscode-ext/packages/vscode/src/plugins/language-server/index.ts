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

const packageJson = require('../../../package.json') // eslint-disable-line

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

const getLatestVersion = async () => {
  const url = 'https://raw.githubusercontent.com/GlooHQ/homebrew-baml/main/version.json'
  console.info('Checking for updates at', url)
  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`Failed to get versions: ${response.status}`)
  }
  const versions = (await response.json()) as { cli: string; py_client: string }

  // Parse as semver
  const cli = semver.parse(versions.cli)
  const py_client = semver.parse(versions.py_client)

  if (!cli || !py_client) {
    throw new Error('Failed to parse versions')
  }

  return { cli, py_client }
}

const getCheckForUpdates = async ({ showIfNoUpdates }: { showIfNoUpdates: boolean }) => {
  try {
    const [versions, localVersion] = await Promise.allSettled([getLatestVersion(), cliVersion()])

    if (versions.status === 'rejected') {
      vscode.window.showErrorMessage(`Failed to check for updates ${versions.reason}`)
      return
    }

    if (localVersion.status === 'rejected') {
      vscode.window
        .showErrorMessage(`Have you installed BAML? ${localVersion.reason}`, {
          title: 'Install BAML',
        })
        .then((selection) => {
          if (selection?.title === 'Install BAML') {
            // Open a url to: docs.boundaryml.com
            vscode.commands.executeCommand(
              'vscode.open',
              Uri.parse('https://docs.boundaryml.com/v2/mdx/quickstart#install-baml-compiler'),
            )
          }
        })
      return
    }

    let { cli } = versions.value
    let localCli = localVersion.value

    if (semver.gt(cli, localCli)) {
      vscode.window
        .showInformationMessage(
          `A new version of BAML is available. Please update from ${localCli} -> ${cli} by running "baml update" in the terminal.`,
          {
            title: 'Update now',
          },
        )
        .then((selection) => {
          if (selection?.title === 'Update now') {
            // Open a new terminal
            vscode.commands.executeCommand('workbench.action.terminal.new').then(() => {
              // Run the update command
              vscode.commands.executeCommand('workbench.action.terminal.sendSequence', {
                text: 'baml update\n',
              })
            })
          }
        })
    } else {
      if (showIfNoUpdates) {
        vscode.window.showInformationMessage(`BAML ${cli} is up to date!`)
      } else {
        console.info(`BAML is up to date! ${cli} <= ${localCli}`)
      }
    }
  } catch (e) {
    console.error('Failed to check for updates', e)
  }
}

const cliVersion = async (): Promise<semver.SemVer> => {
  const res = await client.sendRequest<string | undefined>('cliVersion')
  if (res) {
    let parsed = semver.parse(res.split(' ').at(-1))
    if (!parsed) {
      throw new Error(`Failed to parse version: ${res}`)
    }
    return parsed
  }
  throw new Error('Failed to get CLI version')
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

let bamlOutputChannel: OutputChannel | null = null
const activateClient = (
  context: ExtensionContext,
  serverOptions: ServerOptions,
  clientOptions: LanguageClientOptions,
) => {
  // Create the language client
  client = createLanguageServer(serverOptions, clientOptions)
  window.showInformationMessage('client activating')
  console.log('client activating')

  client.onReady().then(() => {
    window.showInformationMessage('client onReady')
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
    void getCheckForUpdates({ showIfNoUpdates: false })
    // And check again once every hour
    intervalTimers.push(
      setInterval(async () => {
        console.log(`checking for updates ${new Date()}`)
        await getCheckForUpdates({ showIfNoUpdates: false })
      }, 5 * 1000 /* 1h in milliseconds: min/hr * secs/min * ms/sec */),
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
        getCheckForUpdates({ showIfNoUpdates: true }).catch((e) => {
          console.error('Failed to check for updates', e)
        })
      }),

      vscode.commands.registerCommand('baml.jumpToDefinition', async (args: { sourceFile?: string; name?: string }) => {
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
    console.log('hugga humma choo choo activated')

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
