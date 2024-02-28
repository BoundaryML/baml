"use strict";
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
var __generator = (this && this.__generator) || function (thisArg, body) {
    var _ = { label: 0, sent: function() { if (t[0] & 1) throw t[1]; return t[1]; }, trys: [], ops: [] }, f, y, t, g;
    return g = { next: verb(0), "throw": verb(1), "return": verb(2) }, typeof Symbol === "function" && (g[Symbol.iterator] = function() { return this; }), g;
    function verb(n) { return function (v) { return step([n, v]); }; }
    function step(op) {
        if (f) throw new TypeError("Generator is already executing.");
        while (g && (g = 0, op[0] && (_ = 0)), _) try {
            if (f = 1, y && (t = op[0] & 2 ? y["return"] : op[0] ? y["throw"] || ((t = y["return"]) && t.call(y), 0) : y.next) && !(t = t.call(y, op[1])).done) return t;
            if (y = 0, t) op = [op[0] & 2, t.value];
            switch (op[0]) {
                case 0: case 1: t = op; break;
                case 4: _.label++; return { value: op[1], done: false };
                case 5: _.label++; y = op[1]; op = [0]; continue;
                case 7: op = _.ops.pop(); _.trys.pop(); continue;
                default:
                    if (!(t = _.trys, t = t.length > 0 && t[t.length - 1]) && (op[0] === 6 || op[0] === 2)) { _ = 0; continue; }
                    if (op[0] === 3 && (!t || (op[1] > t[0] && op[1] < t[3]))) { _.label = op[1]; break; }
                    if (op[0] === 6 && _.label < t[1]) { _.label = t[1]; t = op; break; }
                    if (t && _.label < t[2]) { _.label = t[2]; _.ops.push(op); break; }
                    if (t[2]) _.ops.pop();
                    _.trys.pop(); continue;
            }
            op = body.call(thisArg, _);
        } catch (e) { op = [6, e]; y = 0; } finally { f = t = 0; }
        if (op[0] & 5) throw op[1]; return { value: op[0] ? op[1] : void 0, done: true };
    }
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.WebPanelView = void 0;
var vscode_1 = require("vscode");
var getUri_1 = require("../utils/getUri");
var getNonce_1 = require("../utils/getNonce");
var vscode = require("vscode");
var execute_test_1 = require("./execute_test");
var unique_names_generator_1 = require("unique-names-generator");
var language_server_1 = require("../plugins/language-server");
var vscode_uri_1 = require("vscode-uri");
var customConfig = {
    dictionaries: [unique_names_generator_1.adjectives, unique_names_generator_1.colors, unique_names_generator_1.animals],
    separator: '_',
    length: 2,
};
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
var WebPanelView = /** @class */ (function () {
    /**
     * The WebPanelView class private constructor (called only from the render method).
     *
     * @param panel A reference to the webview panel
     * @param extensionUri The URI of the directory containing the extension
     */
    function WebPanelView(panel, extensionUri) {
        var _this = this;
        this._disposables = [];
        this._panel = panel;
        // Set an event listener to listen for when the panel is disposed (i.e. when the user closes
        // the panel or when the panel is closed programmatically)
        this._panel.onDidDispose(function () { return _this.dispose(); }, null, this._disposables);
        // Set the HTML content for the webview panel
        this._panel.webview.html = this._getWebviewContent(this._panel.webview, extensionUri);
        // Set an event listener to listen for messages passed from the webview context
        this._setWebviewMessageListener(this._panel.webview);
        execute_test_1.default.setStdoutListener(function (log) {
            _this._panel.webview.postMessage({
                command: 'test-stdout',
                content: log,
            });
        });
        execute_test_1.default.setTestStateListener(function (testResults) {
            _this._panel.webview.postMessage({
                command: 'test-results',
                content: testResults,
            });
        });
    }
    /**
     * Renders the current webview panel if it exists otherwise a new webview panel
     * will be created and displayed.
     *
     * @param extensionUri The URI of the directory containing the extension.
     */
    WebPanelView.render = function (extensionUri) {
        if (WebPanelView.currentPanel) {
            // If the webview panel already exists reveal it
            WebPanelView.currentPanel._panel.reveal(vscode_1.ViewColumn.Beside);
        }
        else {
            // If a webview panel does not already exist create and show a new one
            var panel = vscode_1.window.createWebviewPanel(
            // Panel view type
            'showHelloWorld', 
            // Panel title
            'BAML Playground', 
            // The editor column the panel should be displayed in
            vscode_1.ViewColumn.Beside, 
            // Extra panel configurations
            {
                // Enable JavaScript in the webview
                enableScripts: true,
                // Restrict the webview to only load resources from the `out` and `web-panel/dist` directories
                localResourceRoots: [vscode_1.Uri.joinPath(extensionUri, 'out'), vscode_1.Uri.joinPath(extensionUri, 'web-panel/dist')],
                retainContextWhenHidden: true,
                enableCommandUris: true,
            });
            WebPanelView.currentPanel = new WebPanelView(panel, extensionUri);
        }
    };
    WebPanelView.prototype.postMessage = function (command, content) {
        this._panel.webview.postMessage({ command: command, content: content });
    };
    /**
     * Cleans up and disposes of webview resources when the webview panel is closed.
     */
    WebPanelView.prototype.dispose = function () {
        WebPanelView.currentPanel = undefined;
        // Dispose of the current webview panel
        this._panel.dispose();
        var config = vscode_1.workspace.getConfiguration();
        config.update('baml.bamlPanelOpen', false, true);
        // Dispose of all disposables (i.e. commands) for the current webview panel
        while (this._disposables.length) {
            var disposable = this._disposables.pop();
            if (disposable) {
                disposable.dispose();
            }
        }
    };
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
    WebPanelView.prototype._getWebviewContent = function (webview, extensionUri) {
        // The CSS file from the React dist output
        var stylesUri = (0, getUri_1.getUri)(webview, extensionUri, ['web-panel', 'dist', 'assets', 'index.css']);
        // The JS file from the React dist output
        var scriptUri = (0, getUri_1.getUri)(webview, extensionUri, ['web-panel', 'dist', 'assets', 'index.js']);
        var nonce = (0, getNonce_1.getNonce)();
        // Tip: Install the es6-string-html VS Code extension to enable code highlighting below
        return /*html*/ "\n      <!DOCTYPE html>\n      <html lang=\"en\">\n        <head>\n          <meta charset=\"UTF-8\" />\n          <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n          <link rel=\"stylesheet\" type=\"text/css\" href=\"".concat(stylesUri, "\">\n          <title>Hello World</title>\n        </head>\n        <body>\n          <div id=\"root\">Waiting for react: ").concat(scriptUri, "</div>\n          <script type=\"module\" nonce=\"").concat(nonce, "\" src=\"").concat(scriptUri, "\"></script>\n        </body>\n      </html>\n    ");
    };
    /**
     * Sets up an event listener to listen for messages passed from the webview context and
     * executes code based on the message that is recieved.
     *
     * @param webview A reference to the extension webview
     * @param context A reference to the extension context
     */
    WebPanelView.prototype._setWebviewMessageListener = function (webview) {
        var _this = this;
        webview.onDidReceiveMessage(function (message) { return __awaiter(_this, void 0, void 0, function () {
            var command, text, _a, testRequest, csvData_1, saveTestRequest, fileName, uri, testInputContent, testFileContent, e_1, removeTestRequest, uri, e_2, span_1, uri, e_3;
            var _b, _c, _d;
            return __generator(this, function (_e) {
                switch (_e.label) {
                    case 0:
                        command = message.command;
                        text = message.text;
                        _a = command;
                        switch (_a) {
                            case 'receiveData': return [3 /*break*/, 1];
                            case 'runTest': return [3 /*break*/, 2];
                            case 'downloadTestResults': return [3 /*break*/, 4];
                            case 'saveTest': return [3 /*break*/, 5];
                            case 'cancelTestRun': return [3 /*break*/, 10];
                            case 'removeTest': return [3 /*break*/, 11];
                            case 'jumpToFile': return [3 /*break*/, 16];
                        }
                        return [3 /*break*/, 20];
                    case 1:
                        // Code that should run in response to the hello message command
                        vscode_1.window.showInformationMessage(text);
                        return [2 /*return*/];
                    case 2:
                        testRequest = message.data;
                        return [4 /*yield*/, execute_test_1.default.runTest(testRequest)];
                    case 3:
                        _e.sent();
                        return [2 /*return*/];
                    case 4:
                        {
                            csvData_1 = message.data;
                            vscode.window.showSaveDialog({
                                filters: {
                                    'CSV': ['csv']
                                }
                            }).then(function (uri) {
                                if (uri) {
                                    vscode.workspace.fs.writeFile(uri, Buffer.from(csvData_1));
                                }
                            });
                        }
                        _e.label = 5;
                    case 5:
                        saveTestRequest = message.data;
                        fileName = void 0;
                        if (typeof saveTestRequest.testCaseName === 'string') {
                            fileName = "".concat(saveTestRequest.testCaseName, ".json");
                        }
                        else if ((_b = saveTestRequest.testCaseName) === null || _b === void 0 ? void 0 : _b.source_file) {
                            fileName = vscode_uri_1.URI.file(saveTestRequest.testCaseName.source_file).path.split('/').pop();
                        }
                        else {
                            fileName = "".concat((0, unique_names_generator_1.uniqueNamesGenerator)(customConfig), ".json");
                        }
                        if (!fileName) {
                            console.log('No file name provided for test' + saveTestRequest.funcName + ' ' + JSON.stringify(saveTestRequest.testCaseName));
                            return [2 /*return*/];
                        }
                        uri = vscode.Uri.joinPath(vscode_uri_1.URI.file(saveTestRequest.root_path), '__tests__', saveTestRequest.funcName, fileName);
                        testInputContent = void 0;
                        if (saveTestRequest.params.type === 'positional') {
                            // Directly use the value if the type is 'positional'
                            try {
                                testInputContent = JSON.parse(saveTestRequest.params.value);
                            }
                            catch (e) {
                                testInputContent = saveTestRequest.params.value;
                            }
                        }
                        else {
                            // Create an object from the entries if the type is not 'positional'
                            testInputContent = Object.fromEntries(saveTestRequest.params.value.map(function (kv) {
                                if (kv.value === undefined || kv.value === null || kv.value === '') {
                                    return [kv.name, null];
                                }
                                var parsed;
                                try {
                                    parsed = JSON.parse(kv.value);
                                }
                                catch (e) {
                                    parsed = kv.value;
                                }
                                return [kv.name, parsed];
                            }));
                        }
                        testFileContent = {
                            input: testInputContent,
                        };
                        _e.label = 6;
                    case 6:
                        _e.trys.push([6, 8, , 9]);
                        return [4 /*yield*/, vscode.workspace.fs.writeFile(uri, Buffer.from(JSON.stringify(testFileContent, null, 2)))];
                    case 7:
                        _e.sent();
                        (_c = WebPanelView.currentPanel) === null || _c === void 0 ? void 0 : _c.postMessage('setDb', Array.from(language_server_1.BamlDB.entries()));
                        return [3 /*break*/, 9];
                    case 8:
                        e_1 = _e.sent();
                        console.log(e_1);
                        return [3 /*break*/, 9];
                    case 9: return [2 /*return*/];
                    case 10:
                        {
                            execute_test_1.default.cancelExistingTestRun();
                            return [2 /*return*/];
                        }
                        _e.label = 11;
                    case 11:
                        removeTestRequest = message.data;
                        uri = vscode.Uri.parse(removeTestRequest.testCaseName.source_file);
                        _e.label = 12;
                    case 12:
                        _e.trys.push([12, 14, , 15]);
                        return [4 /*yield*/, vscode.workspace.fs.delete(uri)];
                    case 13:
                        _e.sent();
                        (_d = WebPanelView.currentPanel) === null || _d === void 0 ? void 0 : _d.postMessage('setDb', Array.from(language_server_1.BamlDB.entries()));
                        return [3 /*break*/, 15];
                    case 14:
                        e_2 = _e.sent();
                        console.log(e_2);
                        return [3 /*break*/, 15];
                    case 15: return [2 /*return*/];
                    case 16:
                        _e.trys.push([16, 18, , 19]);
                        span_1 = message.data;
                        uri = vscode.Uri.parse(span_1.source_file);
                        return [4 /*yield*/, vscode.workspace.openTextDocument(uri).then(function (doc) {
                                var range = new vscode.Range(doc.positionAt(span_1.start), doc.positionAt(span_1.end));
                                vscode.window.showTextDocument(doc, { selection: range, viewColumn: vscode_1.ViewColumn.One });
                            })];
                    case 17:
                        _e.sent();
                        return [3 /*break*/, 19];
                    case 18:
                        e_3 = _e.sent();
                        console.log(e_3);
                        return [3 /*break*/, 19];
                    case 19: return [2 /*return*/];
                    case 20: return [2 /*return*/];
                }
            });
        }); }, undefined, this._disposables);
    };
    return WebPanelView;
}());
exports.WebPanelView = WebPanelView;
