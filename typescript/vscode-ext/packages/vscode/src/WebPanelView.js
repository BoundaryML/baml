"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var vscode_1 = require("vscode");
var getUri_1 = require("./utils/getUri");
var extension_1 = require("./helpers/extension");
var getNonce_1 = require("./utils/getNonce");
var WebPanelView = /** @class */ (function () {
    function WebPanelView(_extensionUri) {
        this._extensionUri = _extensionUri;
        this._disposables = [];
    }
    WebPanelView.getInstance = function (_extensionUri) {
        if (!WebPanelView.instance) {
            WebPanelView.instance = new WebPanelView(_extensionUri);
        }
        return WebPanelView.instance;
    };
    WebPanelView.prototype.resolveWebviewView = function (webviewView, _context, _token) {
        this._view = webviewView;
        this._view.webview.options = {
            enableScripts: true,
            localResourceRoots: [this._extensionUri],
        };
        this._view.webview.html = this._getHtmlForWebview(this._view.webview);
        this._setWebviewMessageListener(this._view.webview);
    };
    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    WebPanelView.prototype.dispose = function () {
        while (this._disposables.length) {
            var disposable = this._disposables.pop();
            if (disposable) {
                disposable.dispose();
            }
        }
    };
    WebPanelView.prototype._getHtmlForWebview = function (webview) {
        var file = 'src/index.tsx';
        var localPort = '3000';
        var localServerUrl = "localhost:".concat(localPort);
        // The CSS file from the React build output
        var stylesUri = (0, getUri_1.getUri)(webview, this._extensionUri, ['web-panel', 'out', 'assets', 'index.css']);
        var scriptUri;
        var isProd = extension_1.Extension.getInstance().isProductionMode;
        if (isProd) {
            scriptUri = (0, getUri_1.getUri)(webview, this._extensionUri, ['web-panel', 'out', 'assets', 'index.js']);
        }
        else {
            scriptUri = "http://".concat(localServerUrl, "/").concat(file);
        }
        var nonce = (0, getNonce_1.getNonce)();
        var reactRefresh = /*html*/ "\n      <script type=\"module\">\n        import RefreshRuntime from \"http://localhost:3000/@react-refresh\"\n        RefreshRuntime.injectIntoGlobalHook(window)\n        window.$RefreshReg$ = () => {}\n        window.$RefreshSig$ = () => (type) => type\n        window.__vite_plugin_react_preamble_installed__ = true\n      </script>";
        var reactRefreshHash = 'sha256-HjGiRduPjIPUqpgYIIsmVtkcLmuf/iR80mv9eslzb4I=';
        var csp = [
            "default-src 'none';",
            "script-src 'unsafe-eval' https://* ".concat(isProd ? "'nonce-".concat(nonce, "'") : "http://".concat(localServerUrl, " http://0.0.0.0:").concat(localPort, " '").concat(reactRefreshHash, "'")),
            "style-src ".concat(webview.cspSource, " 'self' 'unsafe-inline' https://*"),
            "font-src ".concat(webview.cspSource),
            "connect-src https://* ".concat(isProd
                ? ""
                : "ws://".concat(localServerUrl, " ws://0.0.0.0:").concat(localPort, " http://").concat(localServerUrl, " http://0.0.0.0:").concat(localPort)),
        ];
        return /*html*/ "<!DOCTYPE html>\n    <html lang=\"en\">\n      <head>\n        <meta charset=\"UTF-8\" />\n        <meta http-equiv=\"Content-Security-Policy\" content=\"".concat(csp.join('; '), "\">\n        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n        <link rel=\"stylesheet\" type=\"text/css\" href=\"").concat(stylesUri, "\">\n        <title>VSCode React Starter</title>\n      </head>\n      <body>\n        <div id=\"root\"></div>\n        ").concat(isProd ? '' : reactRefresh, "\n        <script type=\"module\" src=\"").concat(scriptUri, "\"></script>\n      </body>\n    </html>");
    };
    /**
     * Sets up an event listener to listen for messages passed from the webview context and
     * executes code based on the message that is recieved.
     *
     * @param webview A reference to the extension webview
     * @param context A reference to the extension context
     */
    WebPanelView.prototype._setWebviewMessageListener = function (webview) {
        webview.onDidReceiveMessage(function (message) {
            var command = message.command;
            var text = message.text;
            switch (command) {
                case 'hello':
                    // Code that should run in response to the hello message command
                    vscode_1.window.showInformationMessage(text);
                    return;
                // Add more switch case statements here as more webview message commands
                // are created within the webview context (i.e. inside media/main.js)
            }
        }, undefined, this._disposables);
    };
    WebPanelView.viewType = 'WebPanelView';
    return WebPanelView;
}());
exports.default = WebPanelView;
