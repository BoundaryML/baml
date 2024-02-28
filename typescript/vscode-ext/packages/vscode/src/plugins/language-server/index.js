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
exports.telemetry = exports.saveFile = exports.generateTestRequest = exports.BamlDB = void 0;
var path = require("path");
var vscode_1 = require("vscode");
var node_1 = require("vscode-languageclient/node");
var telemetryReporter_1 = require("../../telemetryReporter");
var util_1 = require("../../util");
var vscode = require("vscode");
var WebPanelView_1 = require("../../panels/WebPanelView");
var GlooCodeLensProvider_1 = require("../../GlooCodeLensProvider");
var node_fetch_1 = require("node-fetch");
var semver_1 = require("semver");
var packageJson = require('../../../package.json'); // eslint-disable-line
var client;
var serverModule;
var telemetry;
var lastKnownErrorToast;
var isDebugMode = function () { return process.env.VSCODE_DEBUG_MODE === 'true'; };
var isE2ETestOnPullRequest = function () { return process.env.PRISMA_USE_LOCAL_LS === 'true'; };
exports.BamlDB = new Map();
var generateTestRequest = function (test_request) { return __awaiter(void 0, void 0, void 0, function () {
    return __generator(this, function (_a) {
        switch (_a.label) {
            case 0: return [4 /*yield*/, client.sendRequest('generatePythonTests', test_request)];
            case 1: return [2 /*return*/, _a.sent()];
        }
    });
}); };
exports.generateTestRequest = generateTestRequest;
var getLatestVersion = function () { return __awaiter(void 0, void 0, void 0, function () {
    var url, response, versions, cli, py_client;
    return __generator(this, function (_a) {
        switch (_a.label) {
            case 0:
                url = 'https://raw.githubusercontent.com/GlooHQ/homebrew-baml/main/version.json';
                console.info('Checking for updates at', url);
                return [4 /*yield*/, (0, node_fetch_1.default)(url)];
            case 1:
                response = _a.sent();
                if (!response.ok) {
                    throw new Error("Failed to get versions: ".concat(response.status));
                }
                return [4 /*yield*/, response.json()];
            case 2:
                versions = (_a.sent());
                cli = semver_1.default.parse(versions.cli);
                py_client = semver_1.default.parse(versions.py_client);
                if (!cli || !py_client) {
                    throw new Error('Failed to parse versions');
                }
                return [2 /*return*/, { cli: cli, py_client: py_client }];
        }
    });
}); };
var getCheckForUpdates = function (showIfNoUpdates) { return __awaiter(void 0, void 0, void 0, function () {
    var _a, versions, localVersion, cli, localCli;
    return __generator(this, function (_b) {
        switch (_b.label) {
            case 0: return [4 /*yield*/, Promise.allSettled([getLatestVersion(), cliVersion()])];
            case 1:
                _a = _b.sent(), versions = _a[0], localVersion = _a[1];
                if (versions.status === 'rejected') {
                    vscode.window.showErrorMessage("Failed to check for updates ".concat(versions.reason));
                    return [2 /*return*/];
                }
                if (localVersion.status === 'rejected') {
                    vscode.window
                        .showErrorMessage("Have you installed BAML? ".concat(localVersion.reason), {
                        title: 'Install BAML',
                    })
                        .then(function (selection) {
                        if ((selection === null || selection === void 0 ? void 0 : selection.title) === 'Install BAML') {
                            // Open a url to: docs.boundaryml.com
                            vscode.commands.executeCommand('vscode.open', vscode_1.Uri.parse('https://docs.boundaryml.com/v2/mdx/quickstart#install-baml-compiler'));
                        }
                    });
                    return [2 /*return*/];
                }
                cli = versions.value.cli;
                localCli = localVersion.value;
                if (semver_1.default.gt(cli, localCli)) {
                    vscode.window
                        .showInformationMessage("A new version of BAML is available. Please update from ".concat(localCli, " -> ").concat(cli, " by running \"baml update\" in the terminal."), {
                        title: 'Update now',
                    })
                        .then(function (selection) {
                        if ((selection === null || selection === void 0 ? void 0 : selection.title) === 'Update now') {
                            // Open a new terminal
                            vscode.commands.executeCommand('workbench.action.terminal.new').then(function () {
                                // Run the update command
                                vscode.commands.executeCommand('workbench.action.terminal.sendSequence', {
                                    text: 'baml update\n',
                                });
                            });
                        }
                    });
                }
                else {
                    if (showIfNoUpdates) {
                        vscode.window.showInformationMessage("BAML ".concat(cli, " is up to date!"));
                    }
                    else {
                        console.info("BAML is up to date! ".concat(cli, " <= ").concat(localCli));
                    }
                }
                return [2 /*return*/];
        }
    });
}); };
var cliVersion = function () { return __awaiter(void 0, void 0, void 0, function () {
    var res, parsed;
    return __generator(this, function (_a) {
        switch (_a.label) {
            case 0: return [4 /*yield*/, client.sendRequest('cliVersion')];
            case 1:
                res = _a.sent();
                if (res) {
                    parsed = semver_1.default.parse(res.split(' ').at(-1));
                    if (!parsed) {
                        throw new Error("Failed to parse version: ".concat(res));
                    }
                    return [2 /*return*/, parsed];
                }
                throw new Error('Failed to get CLI version');
        }
    });
}); };
var saveFile = function (filepath) { return __awaiter(void 0, void 0, void 0, function () {
    return __generator(this, function (_a) {
        switch (_a.label) {
            case 0: return [4 /*yield*/, client.sendRequest('saveFile', { filepath: filepath })];
            case 1: return [2 /*return*/, _a.sent()];
        }
    });
}); };
exports.saveFile = saveFile;
var sleep = function (time) {
    return new Promise(function (resolve) {
        setTimeout(function () {
            resolve(true);
        }, time);
    });
};
var bamlOutputChannel = null;
var activateClient = function (context, serverOptions, clientOptions) {
    // Create the language client
    client = (0, util_1.createLanguageServer)(serverOptions, clientOptions);
    client.onReady().then(function () {
        client.onNotification('baml/showLanguageServerOutput', function () {
            // need to append line for the show to work for some reason.
            // dont delete this.
            client.outputChannel.appendLine('baml/showLanguageServerOutput');
            client.outputChannel.show();
        });
        client.onNotification('baml/message', function (message) {
            client.outputChannel.appendLine('baml/message' + JSON.stringify(message, null, 2));
            var msg;
            switch (message.type) {
                case 'warn': {
                    msg = vscode_1.window.showWarningMessage(message.message);
                    break;
                }
                case 'info': {
                    vscode_1.window.withProgress({
                        location: vscode.ProgressLocation.Notification,
                        cancellable: false,
                    }, function (progress, token) { return __awaiter(void 0, void 0, void 0, function () {
                        var customCancellationToken;
                        return __generator(this, function (_a) {
                            customCancellationToken = null;
                            return [2 /*return*/, new Promise(function (resolve) { return __awaiter(void 0, void 0, void 0, function () {
                                    var sleepTimeMs, totalSecs, iterations, i, prog;
                                    return __generator(this, function (_a) {
                                        switch (_a.label) {
                                            case 0:
                                                customCancellationToken = new vscode.CancellationTokenSource();
                                                customCancellationToken.token.onCancellationRequested(function () {
                                                    customCancellationToken === null || customCancellationToken === void 0 ? void 0 : customCancellationToken.dispose();
                                                    customCancellationToken = null;
                                                    vscode.window.showInformationMessage('Cancelled the progress');
                                                    resolve(null);
                                                    return;
                                                });
                                                sleepTimeMs = 1000;
                                                totalSecs = 10;
                                                iterations = (totalSecs * 1000) / sleepTimeMs;
                                                i = 0;
                                                _a.label = 1;
                                            case 1:
                                                if (!(i < iterations)) return [3 /*break*/, 4];
                                                prog = (i / iterations) * 100;
                                                // Increment is summed up with the previous value
                                                progress.report({ increment: prog, message: "BAML Client generated!" });
                                                return [4 /*yield*/, sleep(100)];
                                            case 2:
                                                _a.sent();
                                                _a.label = 3;
                                            case 3:
                                                i++;
                                                return [3 /*break*/, 1];
                                            case 4:
                                                resolve(null);
                                                return [2 /*return*/];
                                        }
                                    });
                                }); })];
                        });
                    }); });
                    break;
                }
                case 'error': {
                    vscode_1.window.showErrorMessage(message.message);
                    break;
                }
                default: {
                    throw new Error('Invalid message type');
                }
            }
        });
        client.onRequest('set_database', function (_a) {
            var _b;
            var rootPath = _a.rootPath, db = _a.db;
            try {
                exports.BamlDB.set(rootPath, db);
                GlooCodeLensProvider_1.default.setDB(rootPath, db);
                console.log('set_database');
                (_b = WebPanelView_1.WebPanelView.currentPanel) === null || _b === void 0 ? void 0 : _b.postMessage('setDb', Array.from(exports.BamlDB.entries()));
            }
            catch (e) {
                console.log('Error setting database', e);
            }
        });
        client.onRequest('rm_database', function (root_path) {
            // TODO: Handle errors better. But for now the playground shouldn't break.
            // BamlDB.delete(root_path)
            // WebPanelView.currentPanel?.postMessage('setDb', Array.from(BamlDB.entries()))
        });
        // this will fail otherwise in dev mode if the config where the baml path is hasnt been picked up yet. TODO: pass the config to the server to avoid this.
        setTimeout(function () {
            getCheckForUpdates(false).catch(function (e) {
                console.error('Failed to check for updates', e);
            });
        }, 5000);
    });
    var disposable = client.start();
    // Start the client. This will also launch the server
    context.subscriptions.push(disposable);
};
var onFileChange = function (filepath) {
    console.debug("File ".concat(filepath, " has changed, restarting TS Server."));
    void vscode_1.commands.executeCommand('typescript.restartTsServer');
};
var plugin = {
    name: 'baml-language-server',
    enabled: function () { return true; },
    activate: function (context, outputChannel) { return __awaiter(void 0, void 0, void 0, function () {
        var isDebugOrTest, debugOptions, serverOptions, clientOptions, extensionId, extensionVersion;
        return __generator(this, function (_a) {
            switch (_a.label) {
                case 0:
                    isDebugOrTest = (0, util_1.isDebugOrTestSession)();
                    bamlOutputChannel = outputChannel;
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
                    console.log('debugmode', isDebugMode());
                    // serverModule = context.asAbsolutePath(path.join('../../packages/language-server/dist/src/bin'))
                    serverModule = context.asAbsolutePath(path.join('language-server', 'out', 'bin'));
                    console.log("serverModules: ".concat(serverModule));
                    debugOptions = {
                        execArgv: ['--nolazy', '--inspect=6009'],
                        env: { DEBUG: true },
                    };
                    serverOptions = {
                        run: { module: serverModule, transport: node_1.TransportKind.ipc },
                        debug: {
                            module: serverModule,
                            transport: node_1.TransportKind.ipc,
                            options: debugOptions,
                        },
                    };
                    clientOptions = {
                        // Register the server for baml docs
                        documentSelector: [
                            { scheme: 'file', language: 'baml' },
                            {
                                language: 'json',
                                pattern: '**/baml_src/**',
                            },
                        ],
                        synchronize: {
                            fileEvents: vscode_1.workspace.createFileSystemWatcher('**/baml_src/**/*.{baml,json}'),
                        }
                    };
                    context.subscriptions.push(
                    // when the file watcher settings change, we need to ensure they are applied
                    vscode_1.workspace.onDidChangeConfiguration(function (event) {
                        // if (event.affectsConfiguration('prisma.fileWatcher')) {
                        //   setGenerateWatcher(!!workspace.getConfiguration('baml').get('fileWatcher'));
                        // }
                    }), vscode_1.commands.registerCommand('baml.restartLanguageServer', function () { return __awaiter(void 0, void 0, void 0, function () {
                        return __generator(this, function (_a) {
                            switch (_a.label) {
                                case 0: return [4 /*yield*/, (0, util_1.restartClient)(context, client, serverOptions, clientOptions)];
                                case 1:
                                    client = _a.sent();
                                    vscode_1.window.showInformationMessage('Baml language server restarted.'); // eslint-disable-line @typescript-eslint/no-floating-promises
                                    return [2 /*return*/];
                            }
                        });
                    }); }), vscode_1.commands.registerCommand('baml.checkForUpdates', function () { return __awaiter(void 0, void 0, void 0, function () {
                        return __generator(this, function (_a) {
                            getCheckForUpdates(true).catch(function (e) {
                                console.error('Failed to check for updates', e);
                            });
                            return [2 /*return*/];
                        });
                    }); }), vscode.commands.registerCommand('baml.jumpToDefinition', function (args) { return __awaiter(void 0, void 0, void 0, function () {
                        var sourceFile, name, response, _a, targetUri, targetRange, targetSelectionRange, uri, doc, selection;
                        return __generator(this, function (_b) {
                            switch (_b.label) {
                                case 0:
                                    sourceFile = args.sourceFile, name = args.name;
                                    if (!sourceFile || !name) {
                                        return [2 /*return*/];
                                    }
                                    return [4 /*yield*/, client.sendRequest('getDefinition', { sourceFile: sourceFile, name: name })];
                                case 1:
                                    response = _b.sent();
                                    if (!response) return [3 /*break*/, 4];
                                    _a = response, targetUri = _a.targetUri, targetRange = _a.targetRange, targetSelectionRange = _a.targetSelectionRange;
                                    uri = vscode_1.Uri.parse(targetUri);
                                    return [4 /*yield*/, vscode_1.workspace.openTextDocument(uri)
                                        // go to line
                                    ];
                                case 2:
                                    doc = _b.sent();
                                    selection = new vscode.Selection(targetSelectionRange.start.line, 0, targetSelectionRange.end.line, 0);
                                    return [4 /*yield*/, vscode_1.window.showTextDocument(doc, { selection: selection, viewColumn: vscode_1.ViewColumn.Beside })];
                                case 3:
                                    _b.sent();
                                    _b.label = 4;
                                case 4: return [2 /*return*/];
                            }
                        });
                    }); }));
                    activateClient(context, serverOptions, clientOptions);
                    console.log('activated');
                    if (!!isDebugOrTest) return [3 /*break*/, 2];
                    extensionId = 'Gloo.' + packageJson.name;
                    extensionVersion = packageJson.version;
                    exports.telemetry = telemetry = new telemetryReporter_1.default(extensionId, extensionVersion);
                    context.subscriptions.push(telemetry);
                    return [4 /*yield*/, telemetry.initialize()];
                case 1:
                    _a.sent();
                    if (extensionId === 'Gloo.baml-insider') {
                        // checkForOtherExtension()
                    }
                    _a.label = 2;
                case 2:
                    (0, util_1.checkForMinimalColorTheme)();
                    return [2 /*return*/];
            }
        });
    }); },
    deactivate: function () { return __awaiter(void 0, void 0, void 0, function () {
        return __generator(this, function (_a) {
            if (!client) {
                return [2 /*return*/, undefined];
            }
            if (!(0, util_1.isDebugOrTestSession)()) {
                telemetry.dispose(); // eslint-disable-line @typescript-eslint/no-floating-promises
            }
            return [2 /*return*/, client.stop()];
        });
    }); },
};
exports.default = plugin;
