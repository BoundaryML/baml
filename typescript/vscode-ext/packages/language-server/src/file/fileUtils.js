"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.convertToTextDocument = exports.gatherFiles = void 0;
var path = require("path");
var fs = require("fs");
var vscode_languageserver_textdocument_1 = require("vscode-languageserver-textdocument");
var vscode_uri_1 = require("vscode-uri");
// export function findTopLevelParent(filePath: string) {
//   let currentPath = filePath;
//   let parentDir: string | null = null;
//   while (currentPath !== path.parse(currentPath).root) {
//     currentPath = path.dirname(currentPath);
//     if (path.basename(currentPath) === 'baml_src') {
//       parentDir = currentPath;
//       break;
//     }
//   }
//   if (parentDir !== null) {
//     return parentDir;
//   }
//   return null;
// }
/**
 * Non-recursively gathers files with .baml or .json extensions from a given directory,
 * avoiding processing the same directory more than once.
 *
 * @param {vscode.Uri} uri - The URI of the directory to search.
 * @param {boolean} debug - Flag to enable debug logging.
 * @returns {string[]} - An array of file URIs.
 */
function gatherFiles(uri, debug) {
    if (debug === void 0) { debug = false; }
    var visitedDirs = new Set();
    var dirStack = [];
    var addDir = function (dir) {
        if (!visitedDirs.has(dir.toString())) {
            dirStack.push(dir);
            visitedDirs.add(dir.toString());
        }
    };
    addDir(uri);
    var fileList = [];
    var MAX_DIRS = 1000;
    var iterations = 0;
    var _loop_1 = function () {
        if (iterations > MAX_DIRS) {
            console.error("Max directory limit reached (".concat(MAX_DIRS, ")"));
            throw new Error("Directory failed to load after ".concat(iterations, " iterations"));
        }
        iterations++;
        var currentUri = dirStack.pop();
        var dirPath = currentUri.fsPath;
        try {
            var files = fs.readdirSync(dirPath);
            files.forEach(function (file) {
                var filePath = path.join(dirPath, file);
                var fileUri = vscode_uri_1.URI.file(filePath);
                var fileStat = fs.statSync(filePath);
                if (fileStat.isDirectory()) {
                    addDir(fileUri);
                }
                else if (filePath.endsWith('.baml') || filePath.endsWith('.json')) {
                    fileList.push(fileUri);
                }
            });
        }
        catch (error) {
            console.error("Error reading directory ".concat(dirPath, ": ").concat(error.message));
            throw error;
        }
    };
    while (dirStack.length > 0) {
        _loop_1();
    }
    return fileList;
}
exports.gatherFiles = gatherFiles;
function convertToTextDocument(filePath) {
    var fileContent = fs.readFileSync(filePath.fsPath, 'utf-8');
    var fileExtension = path.extname(filePath.fsPath);
    return vscode_languageserver_textdocument_1.TextDocument.create(filePath.toString(), fileExtension === '.baml' ? 'baml' : 'json', 1, fileContent);
}
exports.convertToTextDocument = convertToTextDocument;
