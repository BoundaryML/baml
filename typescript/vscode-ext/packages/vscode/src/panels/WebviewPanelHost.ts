import type { StringSpan } from '@baml/common'
import { fromIni } from '@aws-sdk/credential-providers' // ES6 import
import { type Disposable, Uri, ViewColumn, type Webview, type WebviewPanel, window, workspace } from 'vscode'
import * as vscode from 'vscode'
import { getNonce } from '../utils/getNonce'
import { getUri } from '../utils/getUri'
import {
  EchoResponse,
  GetBamlSrcResponse,
  LoadEnvRequest,
  GetPlaygroundPortResponse,
  GetVSCodeSettingsResponse,
  GetWebviewUriResponse,
  WebviewToVscodeRpc,
  encodeBuffer,
  LoadEnvResponse,
} from '../vscode-rpc'

import { type Config, adjectives, animals, colors, uniqueNamesGenerator } from 'unique-names-generator'
import { URI } from 'vscode-uri'
import { getCurrentOpenedFile } from '../helpers/get-open-file'
import { bamlConfig, requestDiagnostics } from '../plugins/language-server'
import TelemetryReporter from '../telemetryReporter'
import { exec, fork } from 'child_process'
import { promisify } from 'util'
import { dirname, join } from 'path'
import * as dotenv from 'dotenv'
import * as fs from 'fs'
import { AwsCredentialIdentity } from '@smithy/types'
// import { CredentialsProviderError } from '@aws-sdk/credential-providers'
const customConfig: Config = {
  dictionaries: [adjectives, colors, animals],
  separator: '_',
  length: 2,
}

export const openPlaygroundConfig: { lastOpenedFunction: null | string } = {
  lastOpenedFunction: null,
}

const execAsync = promisify(exec)
const readFileAsync = promisify(fs.readFile)

/**
 * This class manages the state and behavior of HelloWorld webview panels.
 *
 * It contains all the data and methods for:
 *
 * - Creating and rendering HelloWorld webview panels
 * - Properly cleaning up and disposing of webview resources when the panel is closed
 * - Setting the HTML (and by proxy CSS/JavaScript) content of the webview panel
 * - Setting message listeners so data can be passed between the webview and extension
 */
export class WebviewPanelHost {
  public static currentPanel: WebviewPanelHost | undefined
  private readonly _panel: WebviewPanel
  private _disposables: Disposable[] = []
  private _port: () => number

  /**
   * The WebPanelView class private constructor (called only from the render method).
   *
   * @param panel A reference to the webview panel
   * @param extensionUri The URI of the directory containing the extension
   */
  private constructor(
    panel: WebviewPanel,
    extensionUri: Uri,
    portLoader: () => number,
    private reporter?: TelemetryReporter,
  ) {
    this._panel = panel
    this._port = portLoader

    // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
    // the panel or when the panel is closed programmatically)
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables)

    // Set the HTML content for the webview panel
    this._panel.webview.html = this._getWebviewContent(this._panel.webview, extensionUri)

    // Set an event listener to listen for messages passed from the webview context
    this._setWebviewMessageListener(this._panel.webview)
  }

  /**
   * Renders the current webview panel if it exists otherwise a new webview panel
   * will be created and displayed.
   *
   * @param extensionUri The URI of the directory containing the extension.
   */
  public static render(extensionUri: Uri, portLoader: () => number, reporter: TelemetryReporter) {
    if (WebviewPanelHost.currentPanel) {
      // If the webview panel already exists reveal it
      WebviewPanelHost.currentPanel._panel.reveal(ViewColumn.Beside)
    } else {
      // If a webview panel does not already exist create and show a new one
      const panel = window.createWebviewPanel(
        // Panel view type
        'showHelloWorld',
        // Panel title
        'BAML Playground',
        // The editor column the panel should be displayed in
        // process.env.VSCODE_DEBUG_MODE === 'true' ? ViewColumn.Two : ViewColumn.Beside,
        { viewColumn: ViewColumn.Beside, preserveFocus: true },

        // Extra panel configurations
        {
          // Enable JavaScript in the webview
          enableScripts: true,

          // Restrict the webview to only load resources from the `out` and `web-panel/dist` directories
          localResourceRoots: [
            ...(vscode.workspace.workspaceFolders ?? []).map((f) => f.uri),
            Uri.joinPath(extensionUri, 'out'),
            Uri.joinPath(extensionUri, 'web-panel/dist'),
          ],
          retainContextWhenHidden: true,
          enableCommandUris: true,
        },
      )

      WebviewPanelHost.currentPanel = new WebviewPanelHost(panel, extensionUri, portLoader, reporter)
    }
  }

  public postMessage<T>(command: string, content: T) {
    this._panel.webview.postMessage({ command: command, content })
    console.log('postMessage', command, content)
    this.reporter?.sendTelemetryEvent({
      event: `baml.webview.${command}`,
      properties: {},
    })
  }

  /**
   * Cleans up and disposes of webview resources when the webview panel is closed.
   */
  public dispose() {
    WebviewPanelHost.currentPanel = undefined

    // Dispose of the current webview panel
    this._panel.dispose()

    const config = workspace.getConfiguration()
    config.update('baml.bamlPanelOpen', false, true)

    // Dispose of all disposables (i.e. commands) for the current webview panel
    while (this._disposables.length) {
      const disposable = this._disposables.pop()
      if (disposable) {
        disposable.dispose()
      }
    }
  }

  /**
   * Defines and returns the HTML that should be rendered within the webview panel.
   *
   * @remarks This is also the place where references to the React webview dist files
   * are created and inserted into the webview HTML.
   *
   * @param webview A reference to the extension webview
   * @param extensionUri The URI of the directory containing the extension
   * @returns A template string literal containing the HTML that should be
   * rendered within the webview panel
   */
  private _getWebviewContent(webview: Webview, extensionUri: Uri) {
    // The CSS file from the React dist output
    const stylesUri = getUri(webview, extensionUri, ['web-panel', 'dist', 'assets', 'index.css'])
    // The JS file from the React dist output
    const scriptUri = getUri(webview, extensionUri, ['web-panel', 'dist', 'assets', 'index.js'])

    const nonce = getNonce()

    // Tip: Install the es6-string-html VS Code extension to enable code highlighting below
    return /*html*/ `
          <!DOCTYPE html>
          <html lang="en">
            <head>
              <meta charset="UTF-8" />
              <meta name="viewport" content="width=device-width, initial-scale=1.0" />
              <link rel="stylesheet" type="text/css" href="${stylesUri}">
              <title>Hello World</title>
            </head>
            <body>
              <div id="root">Waiting for react: ${scriptUri}</div>
              <script type="module" nonce="${nonce}" src="${scriptUri}"></script>
            </body>
          </html>`
  }

  /**
   * Sets up an event listener to listen for messages passed from the webview context and
   * executes code based on the message that is recieved.
   *
   * @param webview A reference to the extension webview
   * @param context A reference to the extension context
   */
  private _setWebviewMessageListener(webview: Webview) {
    const addProject = async () => {
      await requestDiagnostics()
      console.log('last opened func', openPlaygroundConfig.lastOpenedFunction)
      this.postMessage('select_function', {
        root_path: 'default',
        function_name: openPlaygroundConfig.lastOpenedFunction,
      })
      this.postMessage('baml_cli_version', bamlConfig.cliVersion)
      this.postMessage('baml_settings_updated', bamlConfig)
    }

    webview.onDidReceiveMessage(
      async (
        message:
          | {
              command: 'get_port' | 'add_project' | 'cancelTestRun' | 'removeTest'
            }
          | {
              command: 'set_flashing_regions'
              spans: { file_path: string; start_line: number; start_char: number; end_line: number; end_char: number }[]
            }
          | {
              command: 'jumpToFile'
              span: StringSpan
            }
          | {
              command: 'telemetry'
              meta: {
                action: string
                data: Record<string, unknown>
              }
            }
          | {
              rpcId: number
              data: WebviewToVscodeRpc
            },
      ) => {
        console.log('DEBUG: webview message: ', message)
        if ('command' in message) {
          switch (message.command) {
            case 'add_project':
              console.log('webview add_project')
              addProject()

              return
            case 'jumpToFile': {
              try {
                console.log('jumpToFile', message.span)
                const span = message.span
                // span.source_file is a file:/// URI

                const uri = vscode.Uri.parse(span.source_file)
                await vscode.workspace.openTextDocument(uri).then((doc) => {
                  const range = new vscode.Range(doc.positionAt(span.start), doc.positionAt(span.end))
                  vscode.window.showTextDocument(doc, { selection: range, viewColumn: ViewColumn.One })
                })
              } catch (e: any) {
                console.log(e)
              }
              return
            }
            case 'telemetry': {
              const { action, data } = message.meta
              this.reporter?.sendTelemetryEvent({
                event: `baml.webview.${action}`,
                properties: data,
              })
              return
            }
            case 'set_flashing_regions': {
              // Call the command handler with the spans
              console.log('WEBPANELVIEW set_flashing_regions', message.spans)
              vscode.commands.executeCommand('baml.setFlashingRegions', { spans: message.spans })
              return
            }
          }
        }

        if (!('rpcId' in message)) {
          return
        }

        // console.log('message from webview, after above handlers:', message)
        const vscodeMessage = message.data
        const vscodeCommand = vscodeMessage.vscodeCommand

        // TODO: implement error handling in our RPC framework
        switch (vscodeCommand) {
          case 'ECHO':
            const echoresp: EchoResponse = { message: vscodeMessage.message }
            // also respond with rpc id
            this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: echoresp })
            return
          case 'SET_PROXY_SETTINGS':
            const { proxyEnabled } = vscodeMessage
            const config = vscode.workspace.getConfiguration()
            config.update('baml.enablePlaygroundProxy', proxyEnabled, vscode.ConfigurationTarget.Workspace)
            return
          case 'GET_WEBVIEW_URI':
            // This is 1:1 with the contents of `image.file` in a test file, e.g. given `image { file baml_src://path/to-image.png }`,
            // relpath will be 'baml_src://path/to-image.png'
            const relpath = vscodeMessage.path

            // NB(san): this is a violation of the "never URI.parse rule"
            // (see https://www.notion.so/gloochat/windows-uri-treatment-fe87b22abebb4089945eb8cd1ad050ef)
            // but this relpath is already a file URI, it seems...
            const uriPath = Uri.parse(relpath)
            const uri = this._panel.webview.asWebviewUri(uriPath).toString()

            console.log('GET_WEBVIEW_URI', { vscodeMessage, uri, parsed: uriPath })
            let webviewUriResp: GetWebviewUriResponse = {
              uri,
            }
            if (vscodeMessage.contents) {
              try {
                const contents = await workspace.fs.readFile(uriPath)
                webviewUriResp = {
                  ...webviewUriResp,
                  contents: encodeBuffer(contents),
                }
              } catch (e) {
                webviewUriResp = {
                  ...webviewUriResp,
                  readError: `${e}`,
                }
              }
            }
            this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: webviewUriResp })
            return
          case 'GET_PLAYGROUND_PORT':
            const response: GetPlaygroundPortResponse = {
              port: this._port(),
            }
            this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: response })
            return
          case 'LOAD_ENV':
            ;(async () => {
              try {
                const envVars = await loadEnv(vscodeMessage)
                this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: envVars })
              } catch (error) {
                this._panel.webview.postMessage({
                  rpcId: message.rpcId,
                  rpcMethod: vscodeCommand,
                  data: { error: error },
                })
              }
            })()
            return
          case 'LOAD_AWS_CREDS':
            ;(async () => {
              try {
                const profile = vscodeMessage.profile
                const credentialProvider = fromIni({
                  profile: profile ?? undefined,
                })
                const awsCreds = await credentialProvider()
                this._panel.webview.postMessage({
                  rpcId: message.rpcId,
                  rpcMethod: vscodeCommand,
                  data: { ok: awsCreds },
                })
              } catch (error) {
                console.error('Error loading aws creds:', error)
                if (error instanceof Error) {
                  this._panel.webview.postMessage({
                    rpcId: message.rpcId,
                    rpcMethod: vscodeCommand,
                    data: {
                      error: {
                        ...error,
                        name: error.name,
                        message: error.message,
                      },
                    },
                  })
                } else {
                  this._panel.webview.postMessage({
                    rpcId: message.rpcId,
                    rpcMethod: vscodeCommand,
                    data: { error },
                  })
                }
              }
            })()
            return
          case 'INITIALIZED': // when the playground is initialized and listening for file changes, we should resend all project files.
            // request diagnostics, which updates the runtime and triggers a new project files update.
            addProject()
            console.log('initialized webview')
            this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: { ack: true } })
            return
        }
      },
      undefined,
      this._disposables,
    )
  }
}

const getActiveWorkspacePath = (): string | undefined => {
  const activeDocument = window.activeTextEditor?.document.uri
  if (activeDocument) {
    const activeWorkspace = workspace.getWorkspaceFolder(activeDocument)
    if (activeWorkspace) {
      return activeWorkspace.uri.fsPath
    }
  }
  return workspace.workspaceFolders?.[0]?.uri.fsPath
}

const getEnvVarBlob = async ({ activeWorkspacePath }: { activeWorkspacePath: string }): Promise<string> => {
  const envVarFile: string | undefined = workspace.getConfiguration('baml').get('envVarFile')
  const envVarCommand: string | undefined = workspace.getConfiguration('baml').get('envVarCommand')

  if (envVarFile) {
    return await readFileAsync(join(activeWorkspacePath, envVarFile), 'utf-8')
  }
  if (envVarCommand) {
    const { stdout, stderr } = await execAsync(envVarCommand, {
      cwd: activeWorkspacePath,
      env: {
        workspaceFolder: activeWorkspacePath,
        fileWorkspaceFolder: activeWorkspacePath,
        ...process.env,
      },
      timeout: 10_000, // milliseconds
      windowsHide: true,
    })
    if (stderr) {
      throw new Error(stderr)
    }
    return stdout
  }
  return ''
}

const loadEnv = async (req: LoadEnvRequest): Promise<LoadEnvResponse> => {
  const activeWorkspacePath = getActiveWorkspacePath()
  if (!activeWorkspacePath) {
    console.warn('Failed to choose workspace for resolving env vars')
    return { envVars: {} }
  }

  const envVarBlob = await getEnvVarBlob({ activeWorkspacePath })
  const envVars = dotenv.parse(envVarBlob)

  console.log('env vars loaded', { time: Date.now(), envVars })

  return { envVars }
}
