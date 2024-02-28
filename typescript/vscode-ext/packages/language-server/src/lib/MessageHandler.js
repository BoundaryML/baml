"use strict";
var __assign = (this && this.__assign) || function () {
    __assign = Object.assign || function(t) {
        for (var s, i = 1, n = arguments.length; i < n; i++) {
            s = arguments[i];
            for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p))
                t[p] = s[p];
        }
        return t;
    };
    return __assign.apply(this, arguments);
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.handleDocumentSymbol = exports.handleCodeActions = exports.handleCompletionResolveRequest = exports.handleRenameRequest = exports.handleCompletionRequest = exports.handleHoverRequest = exports.handleDefinitionRequest = exports.handleDocumentFormatting = exports.handleDiagnosticsRequest = exports.handleGenerateTestFile = void 0;
var vscode_languageserver_1 = require("vscode-languageserver");
var lint_1 = require("./wasm/lint");
var generate_test_file_1 = require("./wasm/generate_test_file");
// import format from './prisma-schema-wasm/format'
// import lint from './prisma-schema-wasm/lint'
var ast_1 = require("./ast");
// import { quickFix } from './code-actions'
// import {
//   insertBasicRename,
//   renameReferencesForModelName,
//   isEnumValue,
//   renameReferencesForEnumValue,
//   isValidFieldName,
//   extractCurrentName,
//   mapExistsAlready,
//   insertMapAttribute,
//   renameReferencesForFieldValue,
//   printLogMessage,
//   isRelationField,
//   isBlockName,
// } from './code-actions/rename'
var ast_2 = require("./ast");
function handleGenerateTestFile(documents, linterInput, test_request, onError) {
    var result = (0, generate_test_file_1.default)(__assign(__assign({}, linterInput), { test_request: test_request }), function (errorMessage) {
        if (onError) {
            onError(errorMessage);
        }
    });
    return result;
}
exports.handleGenerateTestFile = handleGenerateTestFile;
function handleDiagnosticsRequest(rootPath, documents, onError) {
    var linterInput = {
        root_path: rootPath.fsPath,
        files: documents.map(function (_a) {
            var path = _a.path, doc = _a.doc;
            return ({
                path: path,
                content: doc.getText(),
            });
        }),
    };
    console.debug("Linting ".concat(linterInput.files.length, " files in ").concat(linterInput.root_path));
    // console.log("linterInput " + JSON.stringify(linterInput, null, 2))
    var res = (0, lint_1.default)(linterInput, function (errorMessage) {
        if (onError) {
            onError(errorMessage);
        }
    });
    // console.log("res " + JSON.stringify(res, null, 2))
    var allDiagnostics = new Map();
    documents.forEach(function (docDetails) {
        var documentDiagnostics = [];
        try {
            var filteredDiagnostics = res.diagnostics.filter(function (diag) { return diag.source_file === docDetails.path; });
            for (var _i = 0, filteredDiagnostics_1 = filteredDiagnostics; _i < filteredDiagnostics_1.length; _i++) {
                var diag = filteredDiagnostics_1[_i];
                var diagnostic = {
                    range: {
                        start: docDetails.doc.positionAt(diag.start),
                        end: docDetails.doc.positionAt(diag.end),
                    },
                    message: diag.text,
                    source: 'baml',
                };
                if (diag.is_warning) {
                    diagnostic.severity = vscode_languageserver_1.DiagnosticSeverity.Warning;
                }
                else {
                    diagnostic.severity = vscode_languageserver_1.DiagnosticSeverity.Error;
                }
                documentDiagnostics.push(diagnostic);
            }
        }
        catch (e) {
            if (e instanceof Error) {
                console.log('Error handling diagnostics' + e.message + ' ' + e.stack);
            }
            onError === null || onError === void 0 ? void 0 : onError(e.message);
        }
        allDiagnostics.set(docDetails.doc.uri, documentDiagnostics);
    });
    return { diagnostics: allDiagnostics, state: res.ok ? res.response : undefined };
}
exports.handleDiagnosticsRequest = handleDiagnosticsRequest;
/**
 * This handler provides the modification to the document to be formatted.
 */
function handleDocumentFormatting(params, document, onError) {
    // const formatted = format(document.getText(), params, onError)
    // return [TextEdit.replace(fullDocumentRange(document), formatted)]
    return [];
}
exports.handleDocumentFormatting = handleDocumentFormatting;
function handleDefinitionRequest(fileCache, document, params) {
    var position = params.position;
    var lines = (0, ast_1.convertDocumentTextToTrimmedLineArray)(document);
    var word = (0, ast_2.getWordAtPosition)(document, position);
    if (word === '') {
        return;
    }
    // TODO: Do block level definitions
    var match = fileCache.define(word);
    if (match) {
        return [
            {
                targetUri: match.uri.toString(),
                targetRange: match.range,
                targetSelectionRange: match.range,
            },
        ];
    }
    return;
}
exports.handleDefinitionRequest = handleDefinitionRequest;
function handleHoverRequest(fileCache, document, params) {
    var position = params.position;
    var lines = (0, ast_1.convertDocumentTextToTrimmedLineArray)(document);
    var word = (0, ast_2.getWordAtPosition)(document, position);
    if (word === '') {
        return;
    }
    var match = fileCache.define(word);
    if (match) {
        if (match.type === 'function') {
            return {
                contents: {
                    kind: 'markdown',
                    value: "**".concat(match.name, "**\n\n(").concat(match.input, ") -> ").concat(match.output),
                },
            };
        }
        return {
            contents: {
                kind: 'markdown',
                value: "**".concat(match.name, "**\n\n").concat(match.type),
            },
        };
    }
    return;
}
exports.handleHoverRequest = handleHoverRequest;
/**
 *
 * This handler provides the initial list of the completion items.
 */
function handleCompletionRequest(params, document, onError) {
    // return prismaSchemaWasmCompletions(params, document, onError) || localCompletions(params, document, onError)
    return undefined;
}
exports.handleCompletionRequest = handleCompletionRequest;
function handleRenameRequest(params, document) {
    return undefined;
}
exports.handleRenameRequest = handleRenameRequest;
/**
 *
 * @param item This handler resolves additional information for the item selected in the completion list.
 */
function handleCompletionResolveRequest(item) {
    return item;
}
exports.handleCompletionResolveRequest = handleCompletionResolveRequest;
function handleCodeActions(params, document, onError) {
    // if (!params.context.diagnostics.length) {
    //   return []
    // }
    // return quickFix(document, params, onError)
    return [];
}
exports.handleCodeActions = handleCodeActions;
function handleDocumentSymbol(fileCache, params, document) {
    // Since baml is global scope, we can just return all the definitions
    return fileCache.definitions
        .filter(function (def) { return def.uri.toString() === document.uri; })
        .map(function (_a) {
        var name = _a.name, range = _a.range, uri = _a.uri, type = _a.type;
        return ({
            kind: {
                class: vscode_languageserver_1.SymbolKind.Class,
                enum: vscode_languageserver_1.SymbolKind.Enum,
                function: vscode_languageserver_1.SymbolKind.Interface,
                client: vscode_languageserver_1.SymbolKind.Object,
            }[type],
            name: name,
            range: range,
            selectionRange: range,
        });
    })
        .sort(function (a, b) {
        // by kind first
        if (a.kind === b.kind) {
            return a.name.localeCompare(b.name);
        }
        // then by name
        return a.kind - b.kind;
    });
}
exports.handleDocumentSymbol = handleDocumentSymbol;
