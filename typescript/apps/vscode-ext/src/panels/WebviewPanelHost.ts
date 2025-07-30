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
import { bamlConfig, requestDiagnostics } from '../plugins/language-server-client'
import TelemetryReporter from '../telemetryReporter'
import { exec, fork } from 'child_process'
import { promisify } from 'util'
import { dirname, join } from 'path'
import * as dotenv from 'dotenv'
import * as fs from 'fs'
import { AwsCredentialIdentity } from '@smithy/types'
import { refreshBamlConfigSingleton } from '../plugins/language-server-client/bamlConfig'
import { GoogleAuth } from 'google-auth-library'
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
  private _isInitialized: boolean = false
  private _pendingCommands: Array<{ command: string; content: any }> = []

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

    console.log('extensionUri', extensionUri)
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
            Uri.joinPath(extensionUri, "dist/playground"),
          ],
          retainContextWhenHidden: true,
          enableCommandUris: true,
        },
      )

      WebviewPanelHost.currentPanel = new WebviewPanelHost(panel, extensionUri, portLoader, reporter)
    }
  }

  public postMessage<T>(command: string, content: T) {
    if (!this._isInitialized && command === 'select_function') {
      // Queue select_function commands until initialized
      this._pendingCommands.push({ command, content })
      return
    }
    
    this._panel.webview.postMessage({ command: command, content })
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

  private getUris(webview: Webview, extensionUri: Uri, isDevelopment: boolean, localServerUrl: string): { stylesUri: string, scriptUri: string } {
    let stylesUri: string
    let scriptUri: string

    if (isDevelopment) {
      // In development, load from Vite dev server
      stylesUri = `http://${localServerUrl}/src/main.css`;
      scriptUri = `http://${localServerUrl}/src/main.tsx`;
    } else {
      // In production, load from dist folder
      stylesUri = getUri(webview, extensionUri, [
        'dist', 'playground', 'dist', 'assets', 'index.css',
      ]).toString();
      scriptUri = getUri(webview, extensionUri, [
        'dist', 'playground', 'dist', 'assets', 'index.js',
      ]).toString();
    }

    return { stylesUri, scriptUri }
  }

  private verifyUris(stylesUri: string, scriptUri: string) {
    const styleUri = Uri.parse(stylesUri);
    const scriptFileUri = Uri.parse(scriptUri);
    try {
      if (!fs.existsSync(styleUri.fsPath)) {
        throw new Error(`Style file not found: ${styleUri.fsPath}`);
      }
      if (!fs.existsSync(scriptFileUri.fsPath)) {
        throw new Error(`Script file not found: ${scriptFileUri.fsPath}`);
      }
    } catch (e) {
      throw new Error(`Required files not found: ${e instanceof Error ? e.message : String(e)}`);
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
    // Development mode enables hot-reload from Vite dev server
    const isDevelopment = process.env.VSCODE_DEBUG_MODE === 'true'
    // Port 3030 is used in debug mode, 5173 is the default Vite port
    const port = isDevelopment ? 3030 : 5173;
    const localPort = port;
    const localServerUrl = `localhost:${localPort}`;

    const { stylesUri, scriptUri } = this.getUris(webview, extensionUri, isDevelopment, localServerUrl)

    const { stylesUri: stylesUri2, scriptUri: scriptUri2 } = this.getUris(webview, extensionUri, false, localServerUrl)
    // always validate production location is present.
    this.verifyUris(stylesUri2, scriptUri2)

    const nonce = getNonce();

    const reactRefresh = /*html*/ `
      <script type="module" nonce="${nonce}">
        import RefreshRuntime from \"http://${localServerUrl}/@react-refresh\"
        RefreshRuntime.injectIntoGlobalHook(window)
        window.$RefreshReg$ = () => {}
        window.$RefreshSig$ = () => (type) => type
        window.__vite_plugin_react_preamble_installed__ = true
      </script>`;

    // This hash must match the hash of the reactRefresh script above
    const reactRefreshHash = 'sha256-HjGiRduPjIPUqpgYIIsmVtkcLmuf/iR80mv9eslzb4I=';

    const csp = [
      `default-src 'none'`,
      `script-src 'unsafe-eval' https://* ${
        isDevelopment
          ? `http://${localServerUrl} http://0.0.0.0:${localPort} 'nonce-${nonce}'`
          : `'nonce-${nonce}'`
      }`,
      `style-src ${webview.cspSource} 'self' 'unsafe-inline' https://*${
        isDevelopment
          ? ` http://${localServerUrl} http://0.0.0.0:${localPort}`
          : ''
      }`,
      `font-src ${webview.cspSource}`,
      `connect-src https://* ${
        isDevelopment
          ? `ws://${localServerUrl} ws://0.0.0.0:${localPort} http://${localServerUrl} http://0.0.0.0:${localPort}`
          : ''
      }`,
      `img-src ${webview.cspSource} https: data:`
    ];

    return /*html*/ `<!DOCTYPE html>
    <html lang="en">
      <head>
        <meta charset="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <link rel="stylesheet" type="text/css" href="${stylesUri}">
        <title>BAML Playground</title>
        <style>
          /* Match playground loading spinner style */
          .baml-loading-container {
            width: 100vw;
            height: 100vh;
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            background: var(--vscode-sideBar-background, #18181b);
          }
          .baml-loading-box {
            max-width: 24rem;
            width: 100%;
            border: 1px solid var(--vscode-panel-border, #333);
            border-radius: 0.5rem;
            background: var(--vscode-editor-background, #23272e);
            padding: 2rem;
            box-shadow: 0 2px 8px rgba(0,0,0,0.08);
            display: flex;
            flex-direction: column;
            align-items: center;
          }
          .baml-spinner {
            width: 2rem;
            height: 2rem;
            border: 2px solid var(--vscode-panel-border, #333);
            border-top: 2px solid var(--vscode-foreground, #fff);
            border-radius: 50%;
            animation: baml-spin 1s linear infinite;
            margin-bottom: 1.5rem;
            box-sizing: border-box;
            transform-origin: center;
            will-change: transform;
            flex-shrink: 0;
          }
          @keyframes baml-spin {
            from { transform: rotate(0deg); }
            to { transform: rotate(360deg); }
          }
          .baml-loading-title {
            font-size: 1.125rem;
            font-weight: 500;
            color: var(--vscode-foreground, #fff);
            margin-bottom: 0.5rem;
            text-align: center;
          }
          .baml-loading-desc {
            font-size: 0.95rem;
            color: var(--vscode-description-foreground, #aaa);
            text-align: center;
          }
        </style>
      </head>
      <body>
        <div id="root">
          <div class="baml-loading-container">
            <div class="baml-loading-box">
              <div class="baml-spinner"></div>
              <div class="baml-loading-title">Loading BAML Playground...</div>
              <div class="baml-loading-desc">Please wait while the playground loads.</div>
            </div>
          </div>
        </div>
        ${isDevelopment ? reactRefresh : ''}
        <script type="module" ${isDevelopment ? '' : `nonce=\"${nonce}\"`} src="${scriptUri}"></script>
      </body>
    </html>`;
  }

  /**
   * Sets up an event listener to listen for messages passed from the webview context and
   * executes code based on the message that is recieved.
   *
   * @param webview A reference to the extension webview
   * @param context A reference to the extension context
   */
  private _setWebviewMessageListener(webview: Webview) {
    console.log('_setWebviewMessageListener')

    const addProject = async () => {
      await requestDiagnostics()
      // console.log('last opened func', openPlaygroundConfig.lastOpenedFunction)
      // if (openPlaygroundConfig.lastOpenedFunction) {
      //   this.postMessage('select_function', {
      //     root_path: 'default',
      //     function_name: openPlaygroundConfig.lastOpenedFunction,
      //   })
      // }
      this.postMessage('baml_cli_version', bamlConfig.cliVersion)
      this.postMessage('baml_settings_updated', bamlConfig)
    }

    vscode.workspace.onDidChangeConfiguration((event) => {
      console.log('*** CLIENT DID CHANGE CONFIGURATION', event)
      if (event.affectsConfiguration('baml')) {
        setTimeout(() => {
          this.postMessage('baml_settings_updated', refreshBamlConfigSingleton())
        }, 1000)
      }
    })

    webview.onDidReceiveMessage(
      async (
        message:
          | {
              command: 'get_port' | 'add_project' | 'cancelTestRun' | 'removeTest'
            }
          | {
              command: 'set_flashing_regions'
              content: {
                spans: {
                  file_path: string
                  start_line: number
                  start_char: number
                  end_line: number
                  end_char: number
                }[]
              }
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
              console.log('WEBPANELVIEW set_flashing_regions', message.content.spans)
              vscode.commands.executeCommand('baml.setFlashingRegions', { content: message.content })
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
            console.log('GET_WEBVIEW_URI', vscodeMessage)
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
            console.log('GET_PLAYGROUND_PORT', this._port(), Date.now())
            const response: GetPlaygroundPortResponse = {
              port: this._port(),
            }
            this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: response })
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
          case 'LOAD_GCP_CREDS':
            ;(async () => {
              try {
                const auth = new GoogleAuth({
                  scopes: ['https://www.googleapis.com/auth/cloud-platform'],
                })

                const client = await auth.getClient()
                const projectId = await auth.getProjectId()

                const tokenResponse = await client.getAccessToken()

                this._panel.webview.postMessage({
                  rpcId: message.rpcId,
                  rpcMethod: vscodeCommand,
                  data: {
                    ok: {
                      accessToken: tokenResponse.token,
                      projectId,
                    },
                  },
                })
              } catch (error) {
                console.error('Error loading gcp creds:', error)
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
            
            // Mark as initialized and process pending commands
            this._isInitialized = true
            for (const pending of this._pendingCommands) {
              console.log('sending pending command', pending.command, pending.content)
              this._panel.webview.postMessage({ command: pending.command, content: pending.content })
            }
            this._pendingCommands = []
            
            this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: { ack: true } })
            return
        }
      },
      undefined,
      this._disposables,
    )
  }
}