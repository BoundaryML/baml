import type { StringSpan, TestFileContent, TestRequest } from '@baml/common'
import { type Disposable, Uri, ViewColumn, type Webview, type WebviewPanel, window, workspace } from 'vscode'
import * as vscode from 'vscode'
import { getNonce } from '../utils/getNonce'
import { getUri } from '../utils/getUri'
import { EchoResponse, GetBamlSrcResponse, GetWebviewUriResponse, WebviewToVscodeRpc, encodeBuffer } from '../rpc'

import { type Config, adjectives, animals, colors, uniqueNamesGenerator } from 'unique-names-generator'
import { URI } from 'vscode-uri'
import { bamlConfig, requestDiagnostics } from '../plugins/language-server'

const customConfig: Config = {
  dictionaries: [adjectives, colors, animals],
  separator: '_',
  length: 2,
}

export const openPlaygroundConfig: { lastOpenedFunction: null | string } = {
  lastOpenedFunction: null,
}

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
export class WebPanelView {
  public static currentPanel: WebPanelView | undefined
  private readonly _panel: WebviewPanel
  private _disposables: Disposable[] = []
  private _port: () => number

  /**
   * The WebPanelView class private constructor (called only from the render method).
   *
   * @param panel A reference to the webview panel
   * @param extensionUri The URI of the directory containing the extension
   */
  private constructor(panel: WebviewPanel, extensionUri: Uri, portLoader: () => number) {
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
  public static render(extensionUri: Uri, portLoader: () => number) {
    if (WebPanelView.currentPanel) {
      // If the webview panel already exists reveal it
      WebPanelView.currentPanel._panel.reveal(ViewColumn.Beside)
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

      WebPanelView.currentPanel = new WebPanelView(panel, extensionUri, portLoader)
    }
  }

  public postMessage(command: string, content: any) {
    this._panel.webview.postMessage({ command: command, content: content })
  }

  /**
   * Cleans up and disposes of webview resources when the webview panel is closed.
   */
  public dispose() {
    WebPanelView.currentPanel = undefined

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
      </html>
    `
  }

  /**
   * Sets up an event listener to listen for messages passed from the webview context and
   * executes code based on the message that is recieved.
   *
   * @param webview A reference to the extension webview
   * @param context A reference to the extension context
   */
  private _setWebviewMessageListener(webview: Webview) {
    webview.onDidReceiveMessage(
      async (message: any) => {
        const command = message.command
        const text = message.text

        switch (command) {
          case 'get_port':
            // Code that should run in response to the hello message command
            console.log(`Sending port from WebPanelView: ${this._port()}`)
            this.postMessage('port_number', {
              port: this._port(),
            })
            return

          case 'add_project':
            ;(async () => {
              await requestDiagnostics()
              console.log('last opened func', openPlaygroundConfig.lastOpenedFunction)
              this.postMessage('select_function', {
                root_path: 'default',
                function_name: openPlaygroundConfig.lastOpenedFunction,
              })
              this.postMessage('baml_cli_version', bamlConfig.cliVersion)
            })()

            return
          case 'receiveData':
            // Code that should run in response to the hello message command
            window.showInformationMessage(text)
            return

          case 'cancelTestRun': {
            // testExecutor.cancelExistingTestRun()
            return
          }
          case 'removeTest': {
            // const removeTestRequest: {
            //   root_path: string
            //   funcName: string
            //   testCaseName: StringSpan
            // } = message.data
            // const uri = vscode.Uri.file(removeTestRequest.testCaseName.source_file)
            // try {
            //   await vscode.workspace.fs.delete(uri)
            //   WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
            // } catch (e: any) {
            //   console.log(e)
            // }
            return
          }
          case 'jumpToFile': {
            try {
              console.log('jumpToFile', message.data)
              const span = message.data as StringSpan
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
        }

        // console.log('message from webview, after above handlers:', message)
        const vscodeMessage = message.data as WebviewToVscodeRpc
        const vscodeCommand = vscodeMessage.vscodeCommand

        // TODO: implement error handling in our RPC framework
        switch (vscodeCommand) {
          case 'ECHO':
            const echoresp: EchoResponse = { message: vscodeMessage.message }
            // also respond with rpc id
            this._panel.webview.postMessage({ rpcId: message.rpcId, rpcMethod: vscodeCommand, data: echoresp })
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
        }
      },
      undefined,
      this._disposables,
    )
  }
}
