import {
  type CancellationToken,
  type Disposable,
  type Uri,
  type Webview,
  type WebviewView,
  type WebviewViewProvider,
  type WebviewViewResolveContext,
  window,
} from 'vscode'
import { Extension } from './helpers/extension'
import { getNonce } from './utils/getNonce'
import { getUri } from './utils/getUri'

class WebPanelView implements WebviewViewProvider {
  public static readonly viewType = 'WebPanelView'
  private static instance: WebPanelView
  private _disposables: Disposable[] = []

  private _view?: WebviewView

  constructor(private readonly _extensionUri: Uri) {}

  public static getInstance(_extensionUri: Uri): WebPanelView {
    if (!WebPanelView.instance) {
      WebPanelView.instance = new WebPanelView(_extensionUri)
    }

    return WebPanelView.instance
  }

  public resolveWebviewView(webviewView: WebviewView, _context: WebviewViewResolveContext, _token: CancellationToken) {
    this._view = webviewView

    this._view.webview.options = {
      enableScripts: true,
      localResourceRoots: [this._extensionUri],
    }

    this._view.webview.html = this._getHtmlForWebview(this._view.webview)
    this._setWebviewMessageListener(this._view.webview)
  }

  /**
   * Cleans up and disposes of webview resources when the webview panel is closed.
   */
  public dispose() {
    while (this._disposables.length) {
      const disposable = this._disposables.pop()
      if (disposable) {
        disposable.dispose()
      }
    }
  }

  private _getHtmlForWebview(webview: Webview) {
    const file = 'src/index.tsx'
    const localPort = '3000'
    const localServerUrl = `localhost:${localPort}`

    // The CSS file from the React build output
    const stylesUri = getUri(webview, this._extensionUri, ['web-panel', 'out', 'assets', 'index.css'])

    let scriptUri
    const isProd = Extension.getInstance().isProductionMode
    if (isProd) {
      scriptUri = getUri(webview, this._extensionUri, ['web-panel', 'out', 'assets', 'index.js'])
    } else {
      scriptUri = `http://${localServerUrl}/${file}`
    }

    const nonce = getNonce()

    const reactRefresh = /*html*/ `
      <script type="module">
        import RefreshRuntime from "http://localhost:3000/@react-refresh"
        RefreshRuntime.injectIntoGlobalHook(window)
        window.$RefreshReg$ = () => {}
        window.$RefreshSig$ = () => (type) => type
        window.__vite_plugin_react_preamble_installed__ = true
      </script>`

    const reactRefreshHash = 'sha256-HjGiRduPjIPUqpgYIIsmVtkcLmuf/iR80mv9eslzb4I='

    const csp = [
      `default-src 'none';`,
      `script-src 'unsafe-eval' https://* ${
        isProd ? `'nonce-${nonce}'` : `http://${localServerUrl} http://0.0.0.0:${localPort} '${reactRefreshHash}'`
      }`,
      `style-src ${webview.cspSource} 'self' 'unsafe-inline' https://*`,
      `font-src ${webview.cspSource}`,
      `connect-src https://* ${
        isProd
          ? ``
          : `ws://${localServerUrl} ws://0.0.0.0:${localPort} http://${localServerUrl} http://0.0.0.0:${localPort}`
      }`,
    ]

    return /*html*/ `<!DOCTYPE html>
    <html lang="en">
      <head>
        <meta charset="UTF-8" />
        <meta http-equiv="Content-Security-Policy" content="${csp.join('; ')}">
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <link rel="stylesheet" type="text/css" href="${stylesUri}">
        <title>VSCode React Starter</title>
      </head>
      <body>
        <div id="root"></div>
        ${isProd ? '' : reactRefresh}
        <script type="module" src="${scriptUri}"></script>
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
    webview.onDidReceiveMessage(
      (message: any) => {
        const command = message.command
        const text = message.text

        switch (command) {
          case 'hello':
            // Code that should run in response to the hello message command
            window.showInformationMessage(text)
            return
          // Add more switch case statements here as more webview message commands
          // are created within the webview context (i.e. inside media/main.js)
        }
      },
      undefined,
      this._disposables,
    )
  }
}

export default WebPanelView
