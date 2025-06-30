import {
  type Disposable,
  type Uri,
  ViewColumn,
  type WebviewPanel,
  workspace,
} from 'vscode';
import * as vscode from 'vscode';
import { getPlaygroundPort } from '../plugins/language-server-client';
import type TelemetryReporter from '../telemetryReporter';
import { getNonce } from '../utils/getNonce';

import packageJson from '../../package.json'; // eslint-disable-line

// Manual debug toggle - set to true for debug mode, false for production
const DEBUG_MODE = false;

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
  public static currentPanel: WebviewPanelHost | undefined;
  private readonly _panel: WebviewPanel;
  private _disposables: Disposable[] = [];
  private _port: () => number;
  private _playgroundPort: number | null = null;

  /**
   * Gets the current playground port
   */
  public get playgroundPort(): number | null {
    return this._playgroundPort;
  }

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
    this._panel = panel;
    this._port = portLoader;

    // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
    // the panel or when the panel is closed programmatically)
    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    // Show initial loading state
    this._showLoadingState();
  }

  /**
   * Updates the playground port and refreshes the webview
   */
  public updatePlaygroundPort(port: number) {
    console.log(`WebviewPanelHost: Updating playground port to ${port}`);
    this._playgroundPort = port;
    this._updateWebviewContent();
  }

  /**
   * Shows the loading state while waiting for the LSP port
   */
  private _showLoadingState() {
    this._panel.webview.html = `<!DOCTYPE html>
        <html>
        <head>
            <style>
                body {
                    background: linear-gradient(135deg, #0a0a0f 0%, #0f0f1a 25%, #1a1a2e 50%, #0f0f1a 75%, #0a0a0f 100%);
                    color: #e8e8e8;
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    height: 100vh;
                    margin: 0;
                    overflow: hidden;
                }
                .loading-container {
                    text-align: center;
                    max-width: 400px;
                    padding: 40px 20px;
                    background: rgba(255, 255, 255, 0.05);
                    border-radius: 16px;
                    backdrop-filter: blur(10px);
                    border: 1px solid rgba(255, 255, 255, 0.1);
                    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
                }
                .logo {
                    font-size: 32px;
                    margin-bottom: 16px;
                    color: #a855f7;
                    text-shadow: 0 0 20px rgba(168, 85, 247, 0.5);
                }
                .spinner {
                    border: 3px solid rgba(168, 85, 247, 0.2);
                    border-top: 3px solid #a855f7;
                    border-radius: 50%;
                    width: 48px;
                    height: 48px;
                    animation: spin 1.2s linear infinite;
                    margin: 0 auto 24px;
                    box-shadow: 0 0 20px rgba(168, 85, 247, 0.3);
                }
                @keyframes spin {
                    0% { transform: rotate(0deg); }
                    100% { transform: rotate(360deg); }
                }
                .title {
                    font-size: 24px;
                    font-weight: 700;
                    margin-bottom: 8px;
                    color: #ffffff;
                    letter-spacing: -0.5px;
                }
                .subtitle {
                    font-size: 16px;
                    opacity: 0.8;
                    margin-bottom: 4px;
                    color: #d1d5db;
                }
                .debug-info {
                    font-size: 12px;
                    opacity: 0.6;
                    margin-top: 20px;
                    padding: 12px;
                    background: rgba(168, 85, 247, 0.1);
                    border-radius: 8px;
                    border: 1px solid rgba(168, 85, 247, 0.2);
                    display: ${DEBUG_MODE ? 'block' : 'none'};
                }
                .debug-info div {
                    margin-bottom: 4px;
                }
                .debug-info div:last-child {
                    margin-bottom: 0;
                }
            </style>
        </head>
        <body>
            <div class="loading-container">
                <div class="spinner"></div>
                <div class="title">BAML Playground</div>
                <div class="subtitle">Waiting on Baml language server...</div>
                ${
                  DEBUG_MODE
                    ? `
                <div class="debug-info">
                    <div>Debug Mode: Active</div>
                    <div>Waiting for LSP port notification</div>
                    <div>Extension Version: ${packageJson.version}</div>
                </div>
                `
                    : ''
                }
            </div>
        </body>
        </html>`;
  }

  /**
   * Updates the webview content with the playground iframe
   */
  private _updateWebviewContent() {
    if (!this._playgroundPort) {
      // Still waiting for port from LSP
      return;
    }

    const nonce = getNonce();

    this._panel.webview.html = `<!DOCTYPE html>
        <html>
        <head>
            <meta http-equiv="Content-type" content="text/html;charset=UTF-8">

            <meta http-equiv="Content-Security-Policy" content="
                default-src 'none';
                font-src data:;
                style-src ${this._panel.webview.cspSource} 'unsafe-inline';
                script-src 'nonce-${nonce}';
                frame-src *;
                ">

            <style>
                body, html {
                    margin: 0;
                    padding: 0;
                    width: 100%;
                    height: 100vh;
                    overflow: hidden;
                    background: linear-gradient(135deg, #0a0a0f 0%, #0f0f1a 25%, #1a1a2e 50%, #0f0f1a 75%, #0a0a0f 100%);
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
                }
                .header {
                    background: rgba(168, 85, 247, 0.1);
                    color: #e8e8e8;
                    padding: 12px 20px;
                    border-bottom: 1px solid rgba(168, 85, 247, 0.2);
                    font-size: 13px;
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    backdrop-filter: blur(10px);
                    display: ${DEBUG_MODE ? 'flex' : 'none'};
                }
                .header-title {
                    display: flex;
                    align-items: center;
                    gap: 10px;
                }
                .status-indicator {
                    width: 10px;
                    height: 10px;
                    border-radius: 50%;
                    background: #a855f7;
                    animation: pulse 2s infinite;
                    box-shadow: 0 0 10px rgba(168, 85, 247, 0.5);
                }
                @keyframes pulse {
                    0% { opacity: 1; transform: scale(1); }
                    50% { opacity: 0.6; transform: scale(1.1); }
                    100% { opacity: 1; transform: scale(1); }
                }
                .iframe-container {
                    height: ${DEBUG_MODE ? 'calc(100vh - 57px)' : '100vh'};
                    position: relative;
                }
                iframe {
                    width: 100%;
                    height: 100%;
                    border: none;
                    display: block;
                }
                .loading {
                    position: absolute;
                    top: 50%;
                    left: 50%;
                    transform: translate(-50%, -50%);
                    color: #e8e8e8;
                    text-align: center;
                    transition: opacity 0.3s ease;
                    background: rgba(168, 85, 247, 0.1);
                    padding: 20px;
                    border-radius: 12px;
                    border: 1px solid rgba(168, 85, 247, 0.2);
                    backdrop-filter: blur(10px);
                }
                .hidden {
                    opacity: 0;
                    pointer-events: none;
                }
                .port-info {
                    background: rgba(168, 85, 247, 0.2);
                    padding: 4px 10px;
                    border-radius: 6px;
                    font-size: 11px;
                    opacity: 0.8;
                    border: 1px solid rgba(168, 85, 247, 0.3);
                }
                .debug-info {
                    font-size: 11px;
                    opacity: 0.6;
                    margin-top: 8px;
                    padding: 8px;
                    background: rgba(168, 85, 247, 0.1);
                    border-radius: 6px;
                    border: 1px solid rgba(168, 85, 247, 0.2);
                }
            </style>
        </head>
        <body>
            <div class="header">
                <div class="header-title">
                    <div class="status-indicator"></div>
                    <span>BAML Playground (LSP Mode)</span>
                </div>
                <div class="port-info">Port: ${this._playgroundPort}</div>
            </div>
            <div class="iframe-container">
                <div class="loading" id="loading">
                    <p>Connecting to playground...</p>
                    ${
                      DEBUG_MODE
                        ? `
                    <p style="font-size: 12px; opacity: 0.7;">http://localhost:${this._playgroundPort}</p>
                    <div class="debug-info">
                        <div>Debug Mode: Active</div>
                        <div>Connected to port: ${this._playgroundPort}</div>
                        <div>Extension Version: ${packageJson.version}</div>
                    </div>
                    `
                        : ''
                    }
                </div>
                <iframe
                    id="playground"
                    sandbox="allow-scripts allow-forms allow-same-origin"
                    src="http://localhost:${this._playgroundPort}/"
                ></iframe>
            </div>

            <script nonce="${nonce}">
                const iframe = document.getElementById('playground');
                const loading = document.getElementById('loading');

                // Hide loading indicator when iframe loads
                iframe.addEventListener('load', () => {
                    loading.classList.add('hidden');
                    sendVSCodeVars(); // Send theme vars on load
                });

                // Handle navigation attempts (optional)
                iframe.addEventListener('error', () => {
                    loading.innerHTML = '<p style="color: #f87171;">Failed to connect to playground server</p><p style="font-size: 12px;">Make sure the language server is running on port ${this._playgroundPort}</p>';
                    loading.classList.remove('hidden');
                });

                // --- THEME SYNC LOGIC ---
                function sendVSCodeVars() {
                    if (!iframe.contentWindow) return;
                    const styles = getComputedStyle(document.documentElement);
                    // Add more vars as needed
                    const vars = {
                        '--vscode-editor-background': styles.getPropertyValue('--vscode-editor-background'),
                        '--vscode-editor-foreground': styles.getPropertyValue('--vscode-editor-foreground'),
                        '--vscode-editorWidget-background': styles.getPropertyValue('--vscode-editorWidget-background'),
                        '--vscode-editorWidget-foreground': styles.getPropertyValue('--vscode-editorWidget-foreground'),
                        '--vscode-sideBar-background': styles.getPropertyValue('--vscode-sideBar-background'),
                        '--vscode-sideBar-foreground': styles.getPropertyValue('--vscode-sideBar-foreground'),
                        '--vscode-panel-background': styles.getPropertyValue('--vscode-panel-background'),
                        '--vscode-panel-foreground': styles.getPropertyValue('--vscode-panel-foreground'),
                        // ... add more as needed
                    };
                    iframe.contentWindow.postMessage({ type: 'vscode-theme', vars }, '*');
                }

                // MutationObserver for style changes
                const observer = new MutationObserver(() => {
                    sendVSCodeVars();
                });
                observer.observe(document.documentElement, {
                    attributes: true,
                    attributeFilter: ['style'],
                    subtree: false,
                });

                // Fallback polling for theme changes
                let lastBg = '';
                setInterval(() => {
                    const bg = getComputedStyle(document.documentElement).getPropertyValue('--vscode-editor-background');
                    if (bg !== lastBg) {
                        lastBg = bg;
                        sendVSCodeVars();
                    }
                }, 500);
                // --- END THEME SYNC LOGIC ---
            </script>
        </body>
        </html>`;
  }

  /**
   * Renders the current webview panel if it exists otherwise a new webview panel
   * will be created and displayed.
   *
   * @param extensionUri The URI of the directory containing the extension.
   */
  public static render(
    extensionUri: Uri,
    portLoader: () => number,
    reporter: TelemetryReporter,
  ) {
    if (WebviewPanelHost.currentPanel) {
      // If the webview panel already exists reveal it
      WebviewPanelHost.currentPanel._panel.reveal(ViewColumn.Beside);

      // Check if we have a port from LSP and update if needed
      const currentPort = getPlaygroundPort();
      if (currentPort && !WebviewPanelHost.currentPanel.playgroundPort) {
        WebviewPanelHost.currentPanel.updatePlaygroundPort(currentPort);
      }
    } else {
      // If a webview panel does not already exist create and show a new one
      const panel = vscode.window.createWebviewPanel(
        // Panel view type
        'showHelloWorld',
        // Panel title
        'BAML Playground',
        // The editor column the panel should be displayed in
        // process.env.VSCODE_DEBUG_MODE === 'true' ? ViewColumn.Two : ViewColumn.Beside,
        { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },

        // Extra panel configurations
        {
          // Enable JavaScript in the webview
          enableScripts: true,

          // Restrict the webview to only load resources from the `out` and `web-panel/dist` directories
          localResourceRoots: [
            ...(vscode.workspace.workspaceFolders ?? []).map((f) => f.uri),
            vscode.Uri.joinPath(extensionUri, 'out'),
            vscode.Uri.joinPath(extensionUri, 'playground/dist'),
          ],
          retainContextWhenHidden: true,
          enableCommandUris: true,
        },
      );

      WebviewPanelHost.currentPanel = new WebviewPanelHost(
        panel,
        extensionUri,
        portLoader,
        reporter,
      );

      // Check if we already have a port from LSP and update immediately
      const currentPort = getPlaygroundPort();
      if (currentPort) {
        WebviewPanelHost.currentPanel.updatePlaygroundPort(currentPort);
      }
    }
  }

  /**
   * Cleans up and disposes of webview resources when the webview panel is closed.
   */
  public dispose() {
    WebviewPanelHost.currentPanel = undefined;

    // Clean up our resources
    this._panel.dispose();

    const config = workspace.getConfiguration();
    config.update('baml.bamlPanelOpen', false, true);

    while (this._disposables.length) {
      const x = this._disposables.pop();
      if (x) {
        x.dispose();
      }
    }
  }
}
