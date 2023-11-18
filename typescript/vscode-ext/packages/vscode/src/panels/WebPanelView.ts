import { Disposable, Webview, WebviewPanel, window, Uri, ViewColumn, workspace } from 'vscode'
import { getUri } from '../utils/getUri'
import { getNonce } from '../utils/getNonce'
import * as vscode from 'vscode'
import * as path from 'path'
import * as fs from 'fs'
import * as os from 'os'
import { exec } from 'child_process'
import net from 'net'
import { createMessageConnection, StreamMessageReader, StreamMessageWriter } from 'vscode-jsonrpc/node'
import { RunTestRequest } from "@baml/common"
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
      WebPanelView.currentPanel._panel.reveal(ViewColumn.One)
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
      (message: any) => {
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
            const testRequest: RunTestRequest = message.data;
            runPythonCode(testRequest, (stdout: string) => {
              this._panel.webview.postMessage({ command: 'testResult', content: stdout })
            });
            return
          }
        }
      },
      undefined,
      this._disposables,
    )
  }
}

function getWorkspaceFolderPath() {
  // Check if there are any workspace folders open
  if (vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders.length > 0) {
    // Get the first workspace folder
    const workspaceFolder = vscode.workspace.workspaceFolders[0]

    // Get the file system path of the workspace folder
    const workspaceFolderPath = workspaceFolder.uri.fsPath

    return workspaceFolderPath
  } else {
    // No workspace folder is open
    vscode.window.showInformationMessage('No workspace folder is open.')
    return null
  }
}

async function runPythonCode(testRequest: RunTestRequest, stdoutReceiver: (stdout: string) => void) {
  try {
    // Create a temporary file path
    const tempFilePath = path.join(os.tmpdir(), 'test_temp.py')

    // we only send 1 test for now
    const firstTestCase = testRequest.cases[0];

    const fnName = firstTestCase.function_name;
    // for now only single-arg
    const input = firstTestCase.input.values[0];
    // TODO handle str input vs objects vs multiarg

    // Write the Python code to the temporary file
    fs.writeFile(
      tempFilePath,
      `
from baml_client import baml

@baml.${fnName}.test
async def test_some_name(self, ${fnName}Impl: I${fnName}Impl):
    output = await ${fnName}Impl("${input}")
    # assert output == Topic.VEHICLE_REGISTRATION
    return output
    `,
      (err) => {
        if (err) {
          vscode.window.showErrorMessage('Error writing to temporary Python file')
        }
      },
    )

    // Get the workspace folder path
    const workspaceFolderPath = getWorkspaceFolderPath()
    if (!workspaceFolderPath) {
      console.log('No workspace folder path')
      return
    }

    await runWithChildProcess(workspaceFolderPath, tempFilePath, stdoutReceiver)

    // Create and show the terminal
    // const terminal = vscode.window.createTerminal('PythonExecution');
    // terminal.show(true);
    // // Check if a Poetry environment should be used
    // if (fs.existsSync(path.join(workspaceFolderPath, 'pyproject.toml'))) {
    //   // Activate Poetry environment
    //   terminal.sendText('poetry shell');
    //   // Give it a moment to activate
    //   await new Promise(resolve => setTimeout(resolve, 3000));
    // }

    // // Execute the Python script
    // terminal.sendText(`python "${tempFilePath}"`);
  } catch (err) {
    vscode.window.showErrorMessage('Error creating or executing temporary Python file')
  }
}

async function runWithChildProcess(workspaceFolderPath: string, tempFilePath: string, stdoutReceiver: (stdout: string) => void) {
  console.log('runWithChildProcess')
  // Determine if the environment is Poetry
  let pythonExecutable = 'python -m pytest'
  if (fs.existsSync(path.join(workspaceFolderPath, 'pyproject.toml'))) {
    pythonExecutable = `poetry run ${pythonExecutable}`
  }

  const server = net.createServer((socket) => {
    console.log('Python script connected')

    socket.on('data', (data) => {
      console.log('Received from Python:', data.toString())
    })

    socket.on('end', () => {
      console.log('Python script disconnected')
    })

    // Send a message to the Python script
    socket.write('Hello from Node.js\n')
  })

  server.listen(0, '127.0.0.1', () => {
    // Start listening on a random available port
    let addr = server.address()
    let port = typeof addr === 'string' ? parseInt(addr.split(':')[1]) : addr?.port

    vscode.window.showInformationMessage(`Listening on port ${port}`)

    // Run the Python script in a child process
    const command = `${pythonExecutable} ${tempFilePath} --pytest-baml-ipc ${port}`

    // Run the Python script in a child process
    // const process = spawn(pythonExecutable, [tempFilePath]);
    // Run the Python script using exec
    const execOptions = {
      cwd: workspaceFolderPath,
    }

    // Run the Python script using exec, with the specified working directory
    const cp = exec(command, execOptions)
    cp.stdout?.on('data', (data) => {
      console.log(`stream stdout: ${data}`)
      stdoutReceiver(data.toString());
      vscode.window.showInformationMessage(`stream stdout: ${data}`)
    })

    cp.stderr?.on('data', (data) => {
      console.log(`stream stderr: ${data}`)
      stdoutReceiver(data.toString());
      vscode.window.showErrorMessage(`stderr: ${data}`)
    })

    cp.on('close', (code) => {
      console.log(`child process exited with code ${code}`)
      stdoutReceiver(`\nchild process exited with code ${code}`);
      vscode.window.showInformationMessage(`child process exited with code ${code}`)
      server.close()
    })
  })
}
