import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn, workspace } from 'vscode'
import { getUri } from '../utils/getUri'
import { getNonce } from '../utils/getNonce'
import * as vscode from 'vscode'
import { StringSpan, TestFileContent, TestRequest } from '@baml/common'
import testExecutor from './execute_test'

import { uniqueNamesGenerator, Config, adjectives, colors, animals } from 'unique-names-generator'
import { BamlDB, registerFileChange } from '../plugins/language-server'
import { URI } from "vscode-uri";

const customConfig: Config = {
  dictionaries: [adjectives, colors, animals],
  separator: '_',
  length: 2,
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

  /**
   * The WebPanelView class private constructor (called only from the render method).
   *
   * @param panel A reference to the webview panel
   * @param extensionUri The URI of the directory containing the extension
   */
  private constructor(panel: WebviewPanel, extensionUri: Uri) {
    this._panel = panel

    // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
    // the panel or when the panel is closed programmatically)
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables)

    // Set the HTML content for the webview panel
    this._panel.webview.html = this._getWebviewContent(this._panel.webview, extensionUri)

    // Set an event listener to listen for messages passed from the webview context
    this._setWebviewMessageListener(this._panel.webview)
    testExecutor.setStdoutListener((log) => {
      this._panel.webview.postMessage({
        command: 'test-stdout',
        content: log,
      })
    })

    testExecutor.setTestStateListener((testResults) => {
      this._panel.webview.postMessage({
        command: 'test-results',
        content: testResults,
      })
    })
  }

  /**
   * Renders the current webview panel if it exists otherwise a new webview panel
   * will be created and displayed.
   *
   * @param extensionUri The URI of the directory containing the extension.
   */
  public static render(extensionUri: Uri) {
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
        ViewColumn.Beside,
        // Extra panel configurations
        {
          // Enable JavaScript in the webview
          enableScripts: true,
          // Restrict the webview to only load resources from the `out` and `web-panel/dist` directories
          localResourceRoots: [Uri.joinPath(extensionUri, 'out'), Uri.joinPath(extensionUri, 'web-panel/dist')],
          retainContextWhenHidden: true,
        },
      )

      WebPanelView.currentPanel = new WebPanelView(panel, extensionUri)
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
          case 'receiveData':
            // Code that should run in response to the hello message command
            window.showInformationMessage(text)
            return
          // Add more switch case statements here as more webview message commands
          // are created within the webview context (i.e. inside media/main.js)
          // todo: MULTI TEST
          case 'runTest': {
            const testRequest: { root_path: string; tests: TestRequest } = message.data
            await testExecutor.runTest(testRequest)
            return
          }
          case 'saveTest': {
            const saveTestRequest: {
              root_path: string
              funcName: string
              testCaseName: StringSpan | undefined
              params: TestRequest['functions'][0]['tests'][0]['params']
            } = message.data

            const uri = saveTestRequest.testCaseName?.source_file
              ? URI.file(saveTestRequest.testCaseName?.source_file)
              : vscode.Uri.joinPath(
                URI.file(saveTestRequest.root_path),
                '__tests__',
                saveTestRequest.funcName,
                `${uniqueNamesGenerator(customConfig)}.json`,
              )
            let testInputContent: any


            if (saveTestRequest.params.type === 'positional') {
              // Directly use the value if the type is 'positional'
              try {
                testInputContent = JSON.parse(saveTestRequest.params.value)
              } catch (e) {
                testInputContent = saveTestRequest.params.value
              }
            } else {
              // Create an object from the entries if the type is not 'positional'
              testInputContent = Object.fromEntries(
                saveTestRequest.params.value.map((kv: { name: any; value: any }) => {
                  if (kv.value === undefined || kv.value === null || kv.value === '') {
                    return [kv.name, null]
                  }
                  let parsed: any
                  try {
                    parsed = JSON.parse(kv.value)
                  } catch (e) {
                    parsed = kv.value
                  }
                  console.log('parsed k' + kv.name + ' ', JSON.stringify(parsed))
                  return [kv.name, parsed]
                }),
              )
            }

            const testFileContent: TestFileContent = {
              input: testInputContent,
            }
            try {
              await vscode.workspace.fs.writeFile(uri, Buffer.from(JSON.stringify(testFileContent, null, 2)))
              await registerFileChange(uri.toString(), 'json')
              WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
            } catch (e: any) {
              console.log(e)
            }
            return
          }
          case 'removeTest': {
            const removeTestRequest: {
              root_path: string
              funcName: string
              testCaseName: StringSpan
            } = message.data
            const uri = vscode.Uri.parse(removeTestRequest.testCaseName.source_file)
            try {
              await vscode.workspace.fs.delete(uri)
              await registerFileChange(uri.toString(), 'json')
              WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
            } catch (e: any) {
              console.log(e)
            }
            return
          }
          case 'jumpToFile': {
            try {
              const span = message.data as StringSpan
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
      },
      undefined,
      this._disposables,
    )
  }
}

// function getWorkspaceFolderPath() {
//   // Check if there are any workspace folders open
//   if (vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders.length > 0) {
//     // Get the first workspace folder
//     const workspaceFolder = vscode.workspace.workspaceFolders[0]

//     // Get the file system path of the workspace folder
//     const workspaceFolderPath = workspaceFolder.uri.fsPath

//     return workspaceFolderPath
//   } else {
//     // No workspace folder is open
//     vscode.window.showInformationMessage('No workspace folder is open.')
//     return null
//   }
// }
