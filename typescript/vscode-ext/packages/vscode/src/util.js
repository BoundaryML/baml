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
exports.bamlTestShell = exports.bamlPath = exports.restartClient = exports.createLanguageServer = exports.applySnippetWorkspaceEdit = exports.isSnippetEdit = exports.checkForMinimalColorTheme = exports.checkForOtherPrismaExtension = exports.isDebugOrTestSession = void 0;
var vscode_1 = require("vscode");
var node_1 = require("vscode-languageclient/node");
var os_1 = require("os");
var fs_1 = require("fs");
var path_1 = require("path");
function isDebugOrTestSession() {
    return vscode_1.env.sessionId === 'someValue.sessionId';
}
exports.isDebugOrTestSession = isDebugOrTestSession;
function checkForOtherPrismaExtension() {
    var files = (0, fs_1.readdirSync)(path_1.default.join((0, os_1.homedir)(), '.vscode/extensions')).filter(function (file) { return file.toLowerCase().startsWith('Gloo.baml-') && !file.toLowerCase().startsWith('Gloo.baml-insider-'); });
    if (files.length !== 0) {
        // eslint-disable-next-line @typescript-eslint/no-floating-promises
        vscode_1.window.showInformationMessage("You have both both versions (Insider and Stable) of the Baml VS Code extension enabled in your workspace. Please uninstall or disable one of them for a better experience.");
        console.log('Both versions (Insider and Stable) of the Baml VS Code extension are enabled.');
    }
}
exports.checkForOtherPrismaExtension = checkForOtherPrismaExtension;
function showToastToSwitchColorTheme(currentTheme, suggestedTheme) {
    // We do not want to block on this UI message, therefore disabling the linter here.
    // eslint-disable-next-line @typescript-eslint/no-floating-promises
    vscode_1.window.showWarningMessage("The VS Code Color Theme '".concat(currentTheme, "' you are using unfortunately does not fully support syntax highlighting. We suggest you switch to '").concat(suggestedTheme, "' which does fully support it and will give you a better experience."));
}
function checkForMinimalColorTheme() {
    var colorTheme = vscode_1.workspace.getConfiguration('workbench').get('colorTheme');
    if (!colorTheme) {
        return;
    }
    console.log(colorTheme);
    // if (denyListDarkColorThemes.includes(colorTheme as string)) {
    //   showToastToSwitchColorTheme(colorTheme as string, 'Dark+ (Visual Studio)')
    // }
    // if (denyListLightColorThemes.includes(colorTheme as string)) {
    //   showToastToSwitchColorTheme(colorTheme as string, 'Light+ (Visual Studio)')
    // }
}
exports.checkForMinimalColorTheme = checkForMinimalColorTheme;
/* This function is part of the workaround for https://github.com/prisma/language-tools/issues/311 */
function isSnippetEdit(action, document) {
    var _a;
    var changes = (_a = action.edit) === null || _a === void 0 ? void 0 : _a.changes;
    if (changes !== undefined && changes[document.uri]) {
        if (changes[document.uri].some(function (e) { return e.newText.includes('{\n\n}\n'); })) {
            return true;
        }
    }
    return false;
}
exports.isSnippetEdit = isSnippetEdit;
/* This function is part of the workaround for https://github.com/prisma/language-tools/issues/311 */
function applySnippetWorkspaceEdit() {
    var _this = this;
    return function (edit) { return __awaiter(_this, void 0, void 0, function () {
        var _a, uri, edits, editor, editWithSnippet, lineDelta, snip, range;
        return __generator(this, function (_b) {
            switch (_b.label) {
                case 0:
                    _a = edit.entries()[0], uri = _a[0], edits = _a[1];
                    editor = vscode_1.window.visibleTextEditors.find(function (it) { return it.document.uri.toString() === uri.toString(); });
                    if (!editor)
                        return [2 /*return*/];
                    editWithSnippet = undefined;
                    lineDelta = 0;
                    return [4 /*yield*/, editor.edit(function (builder) {
                            for (var _i = 0, edits_1 = edits; _i < edits_1.length; _i++) {
                                var indel = edits_1[_i];
                                if (indel.newText.includes('$0')) {
                                    editWithSnippet = indel;
                                }
                                else if (indel.newText.includes('{\n\n}')) {
                                    indel.newText = indel.newText.replace('{\n\n}', '{\n\t$0\n}');
                                    editWithSnippet = indel;
                                }
                                else {
                                    if (!editWithSnippet) {
                                        lineDelta = (indel.newText.match(/\n/g) || []).length - (indel.range.end.line - indel.range.start.line);
                                    }
                                    builder.replace(indel.range, indel.newText);
                                }
                            }
                        })];
                case 1:
                    _b.sent();
                    if (!editWithSnippet) return [3 /*break*/, 3];
                    snip = editWithSnippet;
                    range = snip.range.with(snip.range.start.with(snip.range.start.line + lineDelta), snip.range.end.with(snip.range.end.line + lineDelta));
                    return [4 /*yield*/, editor.insertSnippet(new vscode_1.SnippetString(snip.newText), range)];
                case 2:
                    _b.sent();
                    _b.label = 3;
                case 3: return [2 /*return*/];
            }
        });
    }); };
}
exports.applySnippetWorkspaceEdit = applySnippetWorkspaceEdit;
function createLanguageServer(serverOptions, clientOptions) {
    return new node_1.LanguageClient('baml', 'Baml Language Server', serverOptions, clientOptions);
}
exports.createLanguageServer = createLanguageServer;
var restartClient = function (context, client, serverOptions, clientOptions) { return __awaiter(void 0, void 0, void 0, function () {
    var _a;
    return __generator(this, function (_b) {
        switch (_b.label) {
            case 0:
                (_a = client === null || client === void 0 ? void 0 : client.diagnostics) === null || _a === void 0 ? void 0 : _a.dispose();
                if (!client) return [3 /*break*/, 2];
                return [4 /*yield*/, client.stop()];
            case 1:
                _b.sent();
                _b.label = 2;
            case 2:
                client = createLanguageServer(serverOptions, clientOptions);
                context.subscriptions.push(client.start());
                return [4 /*yield*/, client.onReady()];
            case 3:
                _b.sent();
                return [2 /*return*/, client];
        }
    });
}); };
exports.restartClient = restartClient;
var bamlPath = function (_a) {
    var _b;
    var _c = _a.for_test, for_test = _c === void 0 ? false : _c;
    var config = vscode_1.workspace.getConfiguration().get('baml');
    var path = (_b = config === null || config === void 0 ? void 0 : config.path) !== null && _b !== void 0 ? _b : 'baml';
    if (for_test && (config === null || config === void 0 ? void 0 : config.test_cli_prefix)) {
        path = "".concat(config.test_cli_prefix, " ").concat(path);
    }
    return path;
};
exports.bamlPath = bamlPath;
var bamlTestShell = function () {
    var config = vscode_1.workspace.getConfiguration().get('baml');
    return config === null || config === void 0 ? void 0 : config.test_shell;
};
exports.bamlTestShell = bamlTestShell;
