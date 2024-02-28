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
exports.startServer = void 0;
var vscode_languageserver_1 = require("vscode-languageserver");
var vscode_uri_1 = require("vscode-uri");
var debounce_1 = require("lodash/debounce");
var node_1 = require("vscode-languageserver/node");
var vscode_languageserver_textdocument_1 = require("vscode-languageserver-textdocument");
var MessageHandler = require("./lib/MessageHandler");
var internals_1 = require("./lib/wasm/internals");
var fileCache_1 = require("./file/fileCache");
var baml_cli_1 = require("./baml-cli");
var fs_1 = require("fs");
var packageJson = require('../../package.json'); // eslint-disable-line
function getConnection(options) {
    var connection = options === null || options === void 0 ? void 0 : options.connection;
    if (!connection) {
        connection = process.argv.includes('--stdio')
            ? (0, node_1.createConnection)(process.stdin, process.stdout)
            : (0, node_1.createConnection)(new node_1.IPCMessageReader(process), new node_1.IPCMessageWriter(process));
    }
    return connection;
}
var hasCodeActionLiteralsCapability = false;
var hasConfigurationCapability = true;
var config = null;
/**
 * Starts the language server.
 *
 * @param options Options to customize behavior
 */
function startServer(options) {
    var _this = this;
    console.log('Server-side -- startServer()');
    // Source code: https://github.com/microsoft/vscode-languageserver-node/blob/main/server/src/common/server.ts#L1044
    var connection = getConnection(options);
    console.log = connection.console.log.bind(connection.console);
    console.error = connection.console.error.bind(connection.console);
    console.log('Starting Baml Language Server...');
    var documents = new vscode_languageserver_1.TextDocuments(vscode_languageserver_textdocument_1.TextDocument);
    var bamlCache = new fileCache_1.BamlDirCache();
    connection.onInitialize(function (params) {
        // Logging first...
        var _a, _b, _c;
        connection.console.info(
        // eslint-disable-next-line
        "Extension '".concat(packageJson === null || packageJson === void 0 ? void 0 : packageJson.name, "': ").concat(packageJson === null || packageJson === void 0 ? void 0 : packageJson.version));
        connection.console.info("Using 'baml-wasm': ".concat((0, internals_1.getVersion)()));
        var prismaEnginesVersion = (0, internals_1.getEnginesVersion)();
        // ... and then capabilities of the language server
        var capabilities = params.capabilities;
        hasCodeActionLiteralsCapability = Boolean((_b = (_a = capabilities === null || capabilities === void 0 ? void 0 : capabilities.textDocument) === null || _a === void 0 ? void 0 : _a.codeAction) === null || _b === void 0 ? void 0 : _b.codeActionLiteralSupport);
        hasConfigurationCapability = Boolean((_c = capabilities === null || capabilities === void 0 ? void 0 : capabilities.workspace) === null || _c === void 0 ? void 0 : _c.configuration);
        var result = {
            capabilities: {
                textDocumentSync: vscode_languageserver_1.TextDocumentSyncKind.Full,
                definitionProvider: true,
                documentFormattingProvider: false,
                // completionProvider: {
                //   resolveProvider: false,
                //   triggerCharacters: ['@', '"', '.'],
                // },
                hoverProvider: true,
                renameProvider: false,
                documentSymbolProvider: true,
                codeLensProvider: {
                    resolveProvider: true,
                },
                workspace: {
                    fileOperations: {
                        didCreate: {
                            filters: [
                                {
                                    scheme: 'file',
                                    pattern: {
                                        glob: '**/*.{baml, json}',
                                    },
                                },
                            ],
                        },
                        didDelete: {
                            filters: [
                                {
                                    scheme: 'file',
                                    pattern: {
                                        glob: '**/*.{baml, json}',
                                    },
                                },
                            ],
                        },
                        didRename: {
                            filters: [
                                {
                                    scheme: 'file',
                                    pattern: {
                                        glob: '**/*.{baml, json}',
                                    },
                                },
                            ],
                        },
                    }
                }
            },
        };
        var hasWorkspaceFolderCapability = !!(capabilities.workspace && !!capabilities.workspace.workspaceFolders);
        if (hasWorkspaceFolderCapability) {
            result.capabilities.workspace = {
                workspaceFolders: {
                    supported: true
                }
            };
        }
        // if (hasCodeActionLiteralsCapability) {
        //   result.capabilities.codeActionProvider = {
        //     codeActionKinds: [CodeActionKind.QuickFix],
        //   }
        // }
        return result;
    });
    connection.onInitialized(function () {
        console.log('initialized');
        if (hasConfigurationCapability) {
            // Register for all configuration changes.
            // eslint-disable-next-line @typescript-eslint/no-floating-promises
            connection.client.register(vscode_languageserver_1.DidChangeConfigurationNotification.type);
            connection.client.register(vscode_languageserver_1.DidChangeWatchedFilesNotification.type);
        }
    });
    // The global settings, used when the `workspace/configuration` request is not supported by the client or is not set by the user.
    // This does not apply to VS Code, as this client supports this setting.
    // const defaultSettings: LSSettings = {}
    // let globalSettings: LSSettings = defaultSettings // eslint-disable-line
    // Cache the settings of all open documents
    var documentSettings = new Map();
    var getConfig = function () { return __awaiter(_this, void 0, void 0, function () {
        var configResponse, e_1;
        return __generator(this, function (_a) {
            switch (_a.label) {
                case 0:
                    _a.trys.push([0, 2, , 3]);
                    return [4 /*yield*/, connection.workspace.getConfiguration('baml')];
                case 1:
                    configResponse = _a.sent();
                    console.log('configResponse ' + JSON.stringify(configResponse, null, 2));
                    config = configResponse;
                    return [3 /*break*/, 3];
                case 2:
                    e_1 = _a.sent();
                    if (e_1 instanceof Error) {
                        console.log('Error getting config' + e_1.message + ' ' + e_1.stack);
                    }
                    else {
                        console.log('Error getting config' + e_1);
                    }
                    return [3 /*break*/, 3];
                case 3: return [2 /*return*/];
            }
        });
    }); };
    function getLanguageExtension(uri) {
        var languageExtension = uri.split('.').pop();
        if (!languageExtension) {
            console.log('Could not find language extension for ' + uri);
            return;
        }
        return languageExtension;
    }
    connection.onDidChangeWatchedFiles(function (params) { return __awaiter(_this, void 0, void 0, function () {
        var uri, languageExtension, textDocument;
        return __generator(this, function (_a) {
            console.log('onDidChangeWatchedFiles ' + JSON.stringify(params, null, 2));
            params.changes.forEach(function (change) {
                var uri = change.uri;
                var languageExtension = getLanguageExtension(uri);
                if (!languageExtension) {
                    return;
                }
                var textDocument = vscode_languageserver_textdocument_1.TextDocument.create(uri, languageExtension, 1, '');
                bamlCache.refreshDirectory(textDocument);
            });
            if (params.changes.length > 0) {
                uri = params.changes[0].uri;
                languageExtension = getLanguageExtension(uri);
                if (!languageExtension) {
                    return [2 /*return*/];
                }
                textDocument = vscode_languageserver_textdocument_1.TextDocument.create(uri, languageExtension, 1, '');
                validateTextDocument(textDocument);
            }
            return [2 /*return*/];
        });
    }); });
    connection.onDidChangeConfiguration(function (_change) {
        getConfig();
        if (hasConfigurationCapability) {
            // Reset all cached document settings
            documentSettings.clear();
        }
        else {
            // globalSettings = <LSSettings>(change.settings.prisma || defaultSettings) // eslint-disable-line @typescript-eslint/no-unsafe-member-access
        }
        // Revalidate all open prisma schemas
        documents.all().forEach(debouncedValidateTextDocument); // eslint-disable-line @typescript-eslint/no-misused-promises
    });
    documents.onDidOpen(function (e) {
        try {
            // TODO: revalidate if something changed
            bamlCache.refreshDirectory(e.document);
            bamlCache.addDocument(e.document);
            debouncedValidateTextDocument(e.document);
        }
        catch (e) {
            if (e instanceof Error) {
                console.log('Error opening doc' + e.message + ' ' + e.stack);
            }
            else {
                console.log('Error opening doc' + e);
            }
        }
    });
    // Note: VS Code strips newline characters from the message
    function showErrorToast(errorMessage) {
        connection.window
            .showErrorMessage(errorMessage, {
            title: 'Show Details',
        })
            .then(function (item) {
            if ((item === null || item === void 0 ? void 0 : item.title) === 'Show Details') {
                connection.sendNotification('baml/showLanguageServerOutput');
            }
        });
    }
    function generateTestFile(test_request) {
        try {
            var _a = bamlCache.lastBamlDir, cache = _a.cache, rootPath = _a.root_path;
            if (!rootPath || !cache) {
                console.error('Could not find root path');
                connection.sendNotification('baml/message', {
                    type: 'error',
                    message: 'Could not find a baml_src directory for root path',
                });
                return;
            }
            var srcDocs = cache.getDocuments();
            var linterInput = {
                root_path: rootPath.fsPath,
                files: srcDocs.map(function (_a) {
                    var path = _a.path, doc = _a.doc;
                    return {
                        path: path,
                        content: doc.getText(),
                    };
                }),
            };
            if (srcDocs.length === 0) {
                console.log('No BAML files found in the workspace.');
                connection.sendNotification('baml/message', {
                    type: 'warn',
                    message: 'Unable to find BAML files. See Output panel -> BAML Language Server for more details.',
                });
            }
            var response = MessageHandler.handleGenerateTestFile(srcDocs, linterInput, test_request, showErrorToast);
            if (response.status === 'ok') {
                return response.content;
            }
            else {
                showErrorToast(response.message);
            }
        }
        catch (e) {
            if (e instanceof Error) {
                console.log('Error generating test file' + e.message + ' ' + e.stack);
            }
            else {
                console.log('Error generating test file' + e);
            }
        }
    }
    // TODO: dont actually debounce for now or strange out of sync things happen..
    // so we currently set to 0
    var debouncedSetDb = (0, debounce_1.default)(function (rootPath, db) {
        void connection.sendRequest('set_database', { rootPath: rootPath.fsPath, db: db });
    }, 0, {
        maxWait: 4000,
        leading: true,
        trailing: true,
    });
    function validateTextDocument(textDocument) {
        try {
            var rootPath = bamlCache.getBamlDir(textDocument);
            if (!rootPath) {
                return;
            }
            var srcDocs = bamlCache.getDocuments(textDocument);
            if (srcDocs.length === 0) {
                console.log("No BAML files found in the workspace. ".concat(rootPath));
                connection.sendNotification('baml/message', {
                    type: 'warn',
                    message: "Empty baml_src directory found: ".concat(rootPath.fsPath, ". See Output panel -> BAML Language Server for more details."),
                });
                return;
            }
            var response = MessageHandler.handleDiagnosticsRequest(rootPath, srcDocs, showErrorToast);
            for (var _i = 0, _a = response.diagnostics; _i < _a.length; _i++) {
                var _b = _a[_i], uri = _b[0], diagnosticList = _b[1];
                void connection.sendDiagnostics({ uri: uri, diagnostics: diagnosticList });
            }
            bamlCache.addDatabase(rootPath, response.state);
            if (response.state) {
                var filecache = bamlCache.getFileCache(textDocument);
                if (filecache) {
                    filecache.setDB(response.state);
                }
                else {
                    console.error('Could not find file cache for ' + textDocument.uri);
                }
                debouncedSetDb(rootPath, response.state);
            }
            else {
                void connection.sendRequest('rm_database', rootPath);
            }
        }
        catch (e) {
            if (e instanceof Error) {
                console.log('Error validating doc' + e.message + ' ' + e.stack);
            }
            else {
                console.log('Error validating doc' + e);
            }
        }
    }
    var debouncedValidateTextDocument = (0, debounce_1.default)(validateTextDocument, 400, {
        maxWait: 4000,
        leading: true,
        trailing: true,
    });
    var debouncedValidateCodelens = (0, debounce_1.default)(validateTextDocument, 1000, {
        maxWait: 4000,
        leading: true,
        trailing: true,
    });
    documents.onDidChangeContent(function (change) {
        var textDocument = change.document;
        var rootPath = bamlCache.getBamlDir(textDocument);
        if (!rootPath) {
            console.error('Could not find root path for ' + textDocument.uri);
            connection.sendNotification('baml/message', {
                type: 'error',
                message: 'Could not find a baml_src directory for ' + textDocument.uri.toString(),
            });
            return;
        }
        // add the document to the cache
        // we want to do this since the doc may not be in disk (it's in vscode memory).
        // If we try to just load docs from disk we will have outdated info.
        try {
            bamlCache.addDocument(textDocument);
        }
        catch (e) {
            console.log("Error adding document to cache " + e);
        }
        debouncedValidateTextDocument(textDocument);
    });
    var debouncedCLIBuild = (0, debounce_1.default)(baml_cli_1.cliBuild, 1000, {
        leading: true,
        trailing: true,
    });
    documents.onDidSave(function (change) {
        try {
            var cliPath = (config === null || config === void 0 ? void 0 : config.path) || 'baml';
            var bamlDir = bamlCache.getBamlDir(change.document);
            if (!bamlDir) {
                console.error('Could not find baml_src dir for ' + change.document.uri + '. Make sure your baml files are in baml_src dir');
                return;
            }
            debouncedCLIBuild(cliPath, bamlDir, showErrorToast, function () {
                connection.sendNotification('baml/message', {
                    type: 'info',
                    message: 'Generated BAML client successfully!',
                });
            });
        }
        catch (e) {
            if (e instanceof Error) {
                console.log('Error saving doc' + e.message + ' ' + e.stack);
            }
            else {
                console.log('Error saving doc' + e);
            }
        }
    });
    function getDocument(uri) {
        return documents.get(uri);
    }
    connection.onDefinition(function (params) {
        var _a;
        var doc = getDocument(params.textDocument.uri);
        if (doc) {
            var db = bamlCache.getFileCache(doc);
            if (db) {
                return MessageHandler.handleDefinitionRequest(db, doc, params);
            }
            else if (doc.languageId === 'python') {
                var db_1 = (_a = bamlCache.lastBamlDir) === null || _a === void 0 ? void 0 : _a.cache;
                console.log(" python: ".concat(doc.uri, " files: ").concat(db_1 === null || db_1 === void 0 ? void 0 : db_1.getDocuments().length));
                if (db_1) {
                    return MessageHandler.handleDefinitionRequest(db_1, doc, params);
                }
            }
        }
    });
    // connection.onCompletion((params: CompletionParams) => {
    //   const doc = getDocument(params.textDocument.uri)
    //   if (doc) {
    //     return MessageHandler.handleCompletionRequest(params, doc, showErrorToast)
    //   }
    // })
    // This handler resolves additional information for the item selected in the completion list.
    // connection.onCompletionResolve((completionItem: CompletionItem) => {
    //   return MessageHandler.handleCompletionResolveRequest(completionItem)
    // })
    connection.onHover(function (params) {
        var doc = getDocument(params.textDocument.uri);
        if (doc) {
            var db = bamlCache.getFileCache(doc);
            if (db) {
                return MessageHandler.handleHoverRequest(db, doc, params);
            }
        }
    });
    connection.onCodeLens(function (params) {
        var document = getDocument(params.textDocument.uri);
        var codeLenses = [];
        if (!document) {
            return codeLenses;
        }
        bamlCache.addDocument(document);
        // Must be separate from the other validateText since we don't want to get stale in our code lenses.
        debouncedValidateCodelens(document);
        var db = bamlCache.getParserDatabase(document);
        var docFsPath = vscode_uri_1.URI.parse(document.uri).fsPath;
        var baml_dir = bamlCache.getBamlDir(document);
        if (!db) {
            console.log('No db for ' + document.uri + ". There may be a linter error or out of sync file");
            return codeLenses;
        }
        var functionNames = db.functions.filter(function (x) { return x.name.source_file === docFsPath; }).map(function (f) { return f.name; });
        var position = document.positionAt(0);
        functionNames.forEach(function (name) {
            var range = vscode_languageserver_1.Range.create(document.positionAt(name.start), document.positionAt(name.end));
            var command = {
                title: '▶️ Open Playground',
                command: 'baml.openBamlPanel',
                arguments: [
                    {
                        projectId: (baml_dir === null || baml_dir === void 0 ? void 0 : baml_dir.fsPath) || '',
                        functionName: name.value,
                        showTests: true,
                    },
                ],
            };
            codeLenses.push({
                range: range,
                command: command
            });
        });
        var implNames = db.functions
            .flatMap(function (f) {
            return f.impls.map(function (i) {
                return {
                    value: i.name.value,
                    start: i.name.start,
                    end: i.name.end,
                    source_file: i.name.source_file,
                    prompt_key: i.prompt_key,
                    function: f.name.value,
                };
            });
        })
            .filter(function (x) { return x.source_file === docFsPath; });
        implNames.forEach(function (name) {
            codeLenses.push({
                range: (vscode_languageserver_1.Range.create(document.positionAt(name.start), document.positionAt(name.end))),
                command: {
                    title: '▶️ Open Playground',
                    command: 'baml.openBamlPanel',
                    arguments: [
                        {
                            projectId: (baml_dir === null || baml_dir === void 0 ? void 0 : baml_dir.fsPath) || '',
                            functionName: name.function,
                            implName: name.value,
                            showTests: true,
                        },
                    ],
                }
            });
            codeLenses.push({
                range: vscode_languageserver_1.Range.create(document.positionAt(name.prompt_key.start), document.positionAt(name.prompt_key.end)),
                command: {
                    title: '▶️ Open Live Preview',
                    command: 'baml.openBamlPanel',
                    arguments: [
                        {
                            projectId: (baml_dir === null || baml_dir === void 0 ? void 0 : baml_dir.fsPath) || '',
                            functionName: name.function,
                            implName: name.value,
                            showTests: false,
                        },
                    ],
                },
            });
        });
        var testCases = db.functions
            .flatMap(function (f) {
            return f.test_cases.map(function (t) {
                return {
                    value: t.name.value,
                    start: t.name.start,
                    end: t.name.end,
                    source_file: t.name.source_file,
                    function: f.name.value,
                };
            });
        })
            .filter(function (x) { return x.source_file === docFsPath; });
        testCases.forEach(function (name) {
            var range = vscode_languageserver_1.Range.create(document.positionAt(name.start), document.positionAt(name.end));
            var command = {
                title: '▶️ Open Playground',
                command: 'baml.openBamlPanel',
                arguments: [
                    {
                        projectId: (baml_dir === null || baml_dir === void 0 ? void 0 : baml_dir.fsPath) || '',
                        functionName: name.function,
                        testCaseName: name.value,
                        showTests: true,
                    },
                ],
            };
            codeLenses.push({
                range: range,
                command: command
            });
        });
        return codeLenses;
        // return [];
    });
    // connection.onDocumentFormatting((params: DocumentFormattingParams) => {
    //   const doc = getDocument(params.textDocument.uri)
    //   if (doc) {
    //     return MessageHandler.handleDocumentFormatting(params, doc, showErrorToast)
    //   }
    // })
    // connection.onCodeAction((params: CodeActionParams) => {
    //   const doc = getDocument(params.textDocument.uri)
    //   if (doc) {
    //     return MessageHandler.handleCodeActions(params, doc, showErrorToast)
    //   }
    // })
    // connection.onRenameRequest((params: RenameParams) => {
    //   const doc = getDocument(params.textDocument.uri)
    //   if (doc) {
    //     return MessageHandler.handleRenameRequest(params, doc)
    //   }
    // })
    connection.onDocumentSymbol(function (params) {
        var doc = getDocument(params.textDocument.uri);
        if (doc) {
            var db = bamlCache.getFileCache(doc);
            if (db) {
                var symbols = MessageHandler.handleDocumentSymbol(db, params, doc);
                return symbols;
            }
        }
    });
    connection.onRequest('getDefinition', function (_a) {
        var sourceFile = _a.sourceFile, name = _a.name;
        var fileCache = bamlCache.getCacheForUri(sourceFile);
        if (fileCache) {
            var match = fileCache.define(name);
            if (match) {
                return {
                    targetUri: match.uri.toString(),
                    targetRange: match.range,
                    targetSelectionRange: match.range,
                };
            }
        }
    });
    connection.onRequest('cliVersion', function () { return __awaiter(_this, void 0, void 0, function () {
        var res, e_2;
        return __generator(this, function (_a) {
            switch (_a.label) {
                case 0:
                    console.log('Checking baml version at ' + (config === null || config === void 0 ? void 0 : config.path));
                    _a.label = 1;
                case 1:
                    _a.trys.push([1, 3, , 4]);
                    return [4 /*yield*/, new Promise(function (resolve, reject) {
                            (0, baml_cli_1.cliVersion)((config === null || config === void 0 ? void 0 : config.path) || 'baml', reject, function (ver) {
                                resolve(ver);
                            });
                        })];
                case 2:
                    res = _a.sent();
                    return [2 /*return*/, res];
                case 3:
                    e_2 = _a.sent();
                    if (e_2 instanceof Error) {
                        console.log('Error getting cli version' + e_2.message + ' ' + e_2.stack);
                    }
                    else {
                        console.log('Error getting cli version' + e_2);
                    }
                    return [2 /*return*/, undefined];
                case 4: return [2 /*return*/];
            }
        });
    }); });
    connection.onRequest('generatePythonTests', function (params) {
        return generateTestFile(params);
    });
    connection.onRequest("saveFile", function (params) { return __awaiter(_this, void 0, void 0, function () {
        var uri, document;
        return __generator(this, function (_a) {
            console.log("saveFile" + JSON.stringify(params, null, 2));
            uri = vscode_uri_1.URI.parse(params.filepath);
            document = getDocument(uri.toString());
            if (!document) {
                console.log("Could not find document for " + uri.toString());
                return [2 /*return*/];
            }
            try {
                fs_1.default.writeFileSync(uri.fsPath, document.getText());
            }
            catch (e) {
                console.error("Error writing file " + e);
            }
            return [2 /*return*/];
        });
    }); });
    console.log('Server-side -- listening to connection');
    // Make the text document manager listen on the connection
    // for open, change and close text document events
    documents.listen(connection);
    connection.listen();
}
exports.startServer = startServer;
