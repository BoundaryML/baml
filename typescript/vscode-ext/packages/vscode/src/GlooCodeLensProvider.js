"use strict";
var __spreadArray = (this && this.__spreadArray) || function (to, from, pack) {
    if (pack || arguments.length === 2) for (var i = 0, l = from.length, ar; i < l; i++) {
        if (ar || !(i in from)) {
            if (!ar) ar = Array.prototype.slice.call(from, 0, i);
            ar[i] = from[i];
        }
    }
    return to.concat(ar || Array.prototype.slice.call(from));
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.GlooCodeLensProvider = void 0;
var vscode = require("vscode");
var GlooCodeLensProvider = /** @class */ (function () {
    function GlooCodeLensProvider() {
    }
    GlooCodeLensProvider.prototype.setDB = function (path, db) {
        this.path = path;
        this.db = db;
    };
    GlooCodeLensProvider.prototype.provideCodeLenses = function (document) {
        try {
            // .baml and .json happen in lang server now
            if (document.languageId === 'python') {
                return this.getPythonCodeLenses(document);
            }
            else {
                return [];
            }
        }
        catch (e) {
            console.log("Error providing code lenses" + JSON.stringify(e, null, 2));
            return [];
        }
    };
    GlooCodeLensProvider.prototype.getPythonCodeLenses = function (document) {
        var _this = this;
        var codeLenses = [];
        if (!this.db || !this.path) {
            return codeLenses;
        }
        // Check if we imported baml_client in this file
        var text = document.getText();
        var bamlImport = text.includes('import baml_client') || text.includes('from baml_client');
        if (!bamlImport) {
            return codeLenses;
        }
        // By convention we only import baml as baml or b so then look for all
        // baml.function_name() or b.function_name() calls and also get the range
        var functionCalls = __spreadArray([], text.matchAll(/(baml|b)\.[a-zA-Z0-9_]+/g), true);
        console.log(functionCalls);
        // For each function call, find the function name and then find the
        // function in the db
        functionCalls.forEach(function (match) {
            var _a, _b;
            var call = match[0];
            var position = (_a = match.index) !== null && _a !== void 0 ? _a : 0;
            // get line number
            var line = document.positionAt(position);
            var functionName = call.split('.')[1];
            var functionDef = (_b = _this.db) === null || _b === void 0 ? void 0 : _b.functions.find(function (f) { return f.name.value === functionName; });
            if (functionDef) {
                var range = new vscode.Range(document.positionAt(position), document.positionAt(position + functionName.length));
                var fromArgType = function (arg) {
                    if (arg.arg_type === 'positional') {
                        return "".concat(arg.type);
                    }
                    else {
                        return arg.values.map(function (v) { return "".concat(v.name.value, ": ").concat(v.type); }).join(', ');
                    }
                };
                var command = {
                    title: "\u25B6\uFE0F (".concat(fromArgType(functionDef.input), ") -> ").concat(fromArgType(functionDef.output)),
                    tooltip: 'Open in BAML',
                    command: 'baml.jumpToDefinition',
                    arguments: [
                        {
                            sourceFile: functionDef.name.source_file,
                            name: functionName,
                        },
                    ],
                };
                codeLenses.push(new vscode.CodeLens(range, command));
            }
        });
        return codeLenses;
    };
    return GlooCodeLensProvider;
}());
exports.GlooCodeLensProvider = GlooCodeLensProvider;
exports.default = new GlooCodeLensProvider();
