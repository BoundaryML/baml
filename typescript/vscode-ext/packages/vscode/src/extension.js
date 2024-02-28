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
exports.deactivate = exports.activate = void 0;
/* eslint-disable @typescript-eslint/no-var-requires */
/* eslint-disable @typescript-eslint/no-unsafe-assignment */
var vscode = require("vscode");
var plugins_1 = require("./plugins");
var WebPanelView_1 = require("./panels/WebPanelView");
var language_server_1 = require("./plugins/language-server");
var execute_test_1 = require("./panels/execute_test");
var GlooCodeLensProvider_1 = require("./GlooCodeLensProvider");
var language_server_2 = require("./plugins/language-server");
var outputChannel = vscode.window.createOutputChannel('baml');
var diagnosticsCollection = vscode.languages.createDiagnosticCollection('baml');
var LANG_NAME = 'Baml';
function activate(context) {
    var _this = this;
    var baml_config = vscode.workspace.getConfiguration('baml');
    execute_test_1.default.start();
    var bamlPlygroundCommand = vscode.commands.registerCommand('baml.openBamlPanel', function (args) {
        var _a, _b;
        var projectId = args === null || args === void 0 ? void 0 : args.projectId;
        var initialFunctionName = args === null || args === void 0 ? void 0 : args.functionName;
        var initialImplName = args === null || args === void 0 ? void 0 : args.implName;
        var showTests = args === null || args === void 0 ? void 0 : args.showTests;
        var config = vscode.workspace.getConfiguration();
        config.update('baml.bamlPanelOpen', true, vscode.ConfigurationTarget.Global);
        WebPanelView_1.WebPanelView.render(context.extensionUri);
        language_server_2.telemetry.sendTelemetryEvent({
            event: 'baml.openBamlPanel',
            properties: {},
        });
        (_a = WebPanelView_1.WebPanelView.currentPanel) === null || _a === void 0 ? void 0 : _a.postMessage('setDb', Array.from(language_server_1.BamlDB.entries()));
        // send another request for reliability on slower machines
        // A more resilient way is to get a msg for it to finish loading but this is good enough for now
        setTimeout(function () {
            var _a;
            (_a = WebPanelView_1.WebPanelView.currentPanel) === null || _a === void 0 ? void 0 : _a.postMessage('setDb', Array.from(language_server_1.BamlDB.entries()));
        }, 2000);
        (_b = WebPanelView_1.WebPanelView.currentPanel) === null || _b === void 0 ? void 0 : _b.postMessage('setSelectedResource', {
            projectId: projectId,
            functionName: initialFunctionName,
            implName: initialImplName,
            testCaseName: undefined,
            showTests: showTests,
        });
    });
    context.subscriptions.push(bamlPlygroundCommand);
    context.subscriptions.push(vscode.languages.registerCodeLensProvider({ scheme: 'file', language: 'python' }, GlooCodeLensProvider_1.default));
    plugins_1.default.map(function (plugin) { return __awaiter(_this, void 0, void 0, function () {
        var enabled;
        return __generator(this, function (_a) {
            switch (_a.label) {
                case 0: return [4 /*yield*/, plugin.enabled()];
                case 1:
                    enabled = _a.sent();
                    if (!enabled) return [3 /*break*/, 4];
                    console.log("Activating ".concat(plugin.name));
                    if (!plugin.activate) return [3 /*break*/, 3];
                    return [4 /*yield*/, plugin.activate(context, outputChannel)];
                case 2:
                    _a.sent();
                    _a.label = 3;
                case 3: return [3 /*break*/, 5];
                case 4:
                    console.log("".concat(plugin.name, " is Disabled"));
                    _a.label = 5;
                case 5: return [2 /*return*/];
            }
        });
    }); });
}
exports.activate = activate;
function deactivate() {
    execute_test_1.default.close();
    console.log('deactivate');
    plugins_1.default.forEach(function (plugin) {
        if (plugin.deactivate) {
            void plugin.deactivate();
        }
    });
}
exports.deactivate = deactivate;
