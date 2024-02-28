"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.FileCache = exports.BamlDirCache = void 0;
var path_1 = require("path");
var fileUtils_1 = require("./fileUtils");
var BAML_SRC = 'baml_src';
var vscode_uri_1 = require("vscode-uri");
var ast_1 = require("../lib/ast");
var BamlDirCache = /** @class */ (function () {
    function BamlDirCache() {
        this.cache = new Map();
        this.parserCache = new Map();
        this.__lastBamlDir = null;
    }
    Object.defineProperty(BamlDirCache.prototype, "lastBamlDir", {
        get: function () {
            var _a;
            if (this.__lastBamlDir) {
                return { root_path: this.__lastBamlDir, cache: (_a = this.cache.get(this.__lastBamlDir.toString())) !== null && _a !== void 0 ? _a : null };
            }
            else {
                return { root_path: null, cache: null };
            }
        },
        enumerable: false,
        configurable: true
    });
    BamlDirCache.prototype.getBamlDir = function (textDocument) {
        var uri = vscode_uri_1.URI.parse(textDocument.uri);
        return this.getBamlDirUri(uri);
    };
    BamlDirCache.prototype.getBamlDirUri = function (uri) {
        var MAX_TRIES = 10; // configurable maximum depth
        // Check if the scheme is 'file', return null for non-file schemes
        if (uri.scheme !== 'file') {
            console.error("Unsupported URI scheme ".concat(JSON.stringify(uri.toJSON(), null, 2)));
            return null;
        }
        var currentPath = uri.fsPath;
        var tries = 0;
        while (path_1.default.isAbsolute(currentPath) && tries < MAX_TRIES) {
            if (path_1.default.basename(currentPath) === BAML_SRC) {
                return vscode_uri_1.URI.file(currentPath);
            }
            currentPath = path_1.default.dirname(currentPath);
            tries++;
        }
        console.error('No baml dir found within the specified depth');
        return null;
    };
    BamlDirCache.prototype.getFileCache = function (textDocument) {
        var _a;
        var key = this.getBamlDir(textDocument);
        if (!key) {
            return null;
        }
        var cache = (_a = this.cache.get(key.toString())) !== null && _a !== void 0 ? _a : null;
        if (cache) {
            this.__lastBamlDir = key;
        }
        return cache;
    };
    BamlDirCache.prototype.getCacheForUri = function (uri) {
        var _a;
        var key = this.getBamlDirUri(vscode_uri_1.URI.parse(uri));
        if (!key) {
            return null;
        }
        var cache = (_a = this.cache.get(key.toString())) !== null && _a !== void 0 ? _a : null;
        if (cache) {
            this.__lastBamlDir = key;
        }
        return cache;
    };
    BamlDirCache.prototype.getParserDatabase = function (textDocument) {
        var key = this.getBamlDir(textDocument);
        if (!key) {
            return undefined;
        }
        return this.parserCache.get(key.toString());
    };
    BamlDirCache.prototype.listDatabases = function () {
        return Array.from(this.parserCache.entries()).map(function (_a) {
            var root_path = _a[0], db = _a[1];
            return ({
                root_path: vscode_uri_1.URI.parse(root_path),
                db: db,
            });
        });
    };
    BamlDirCache.prototype.createFileCacheIfNotExist = function (textDocument) {
        var key = this.getBamlDir(textDocument);
        var fileCache = this.getFileCache(textDocument);
        if (!fileCache && key) {
            fileCache = new FileCache();
            var allFiles = (0, fileUtils_1.gatherFiles)(key);
            allFiles.forEach(function (filePath) {
                var doc = (0, fileUtils_1.convertToTextDocument)(filePath);
                fileCache === null || fileCache === void 0 ? void 0 : fileCache.addFile(doc);
            });
            this.cache.set(key.toString(), fileCache);
        }
        else if (!key) {
            console.error('Could not find parent directory');
        }
        return fileCache;
    };
    BamlDirCache.prototype.refreshDirectory = function (textDocument) {
        try {
            console.log('refreshDirectory');
            var fileCache_1 = this.createFileCacheIfNotExist(textDocument);
            var parentDir = this.getBamlDir(textDocument);
            if (fileCache_1 && parentDir) {
                var allFiles_1 = (0, fileUtils_1.gatherFiles)(parentDir);
                if (allFiles_1.length === 0) {
                    console.error('No files found');
                    // try again with debug to find issues (temporary hack..)
                    (0, fileUtils_1.gatherFiles)(parentDir, true);
                }
                // remove files that are no longer in the directory
                fileCache_1.getDocuments().forEach(function (_a) {
                    var path = _a.path, doc = _a.doc;
                    if (!allFiles_1.find(function (a) { return a.fsPath === path; })) {
                        fileCache_1.removeFile(doc);
                    }
                });
                // add and update
                allFiles_1.forEach(function (filePath) {
                    if (!fileCache_1.getDocument(filePath)) {
                        var doc = (0, fileUtils_1.convertToTextDocument)(filePath);
                        fileCache_1.addFile(doc);
                    }
                    else {
                        // update the cache
                        var doc = (0, fileUtils_1.convertToTextDocument)(filePath);
                        fileCache_1.addFile(doc);
                    }
                });
            }
            else {
                console.error('Could not find parent directory');
            }
        }
        catch (e) {
            if (e instanceof Error) {
                console.log("Error refreshing directory: ".concat(e.message, " ").concat(e.stack));
            }
            else {
                console.log("Error refreshing directory: ".concat(e));
            }
        }
    };
    BamlDirCache.prototype.addDatabase = function (root_dir, database) {
        if (database) {
            this.parserCache.set(root_dir.toString(), database);
        }
        else {
            this.parserCache.delete(root_dir.toString());
        }
    };
    BamlDirCache.prototype.addDocument = function (textDocument) {
        try {
            var fileCache = this.createFileCacheIfNotExist(textDocument);
            fileCache === null || fileCache === void 0 ? void 0 : fileCache.addFile(textDocument);
        }
        catch (e) {
            if (e instanceof Error) {
                console.log("Error adding doc: ".concat(e.message, " ").concat(e.stack));
            }
        }
    };
    BamlDirCache.prototype.removeDocument = function (textDocument) {
        var fileCache = this.getFileCache(textDocument);
        fileCache === null || fileCache === void 0 ? void 0 : fileCache.removeFile(textDocument);
    };
    BamlDirCache.prototype.getDocuments = function (textDocument) {
        var _a;
        var fileCache = this.getFileCache(textDocument);
        return (_a = fileCache === null || fileCache === void 0 ? void 0 : fileCache.getDocuments()) !== null && _a !== void 0 ? _a : [];
    };
    return BamlDirCache;
}());
exports.BamlDirCache = BamlDirCache;
var counter = 0;
var FileCache = /** @class */ (function () {
    function FileCache() {
        this.__definitions = new Map();
        this.cache = new Map();
        this.cacheSummary = new Array();
    }
    FileCache.prototype.addFile = function (textDocument) {
        this.cache.set(textDocument.uri, textDocument);
        this.cacheSummary = Array.from(this.cache).map(function (_a) {
            var uri = _a[0], doc = _a[1];
            return ({
                path: vscode_uri_1.URI.parse(uri).fsPath,
                doc: doc,
            });
        });
    };
    FileCache.prototype.removeFile = function (textDocument) {
        this.cache.delete(textDocument.uri);
        this.cacheSummary = Array.from(this.cache).map(function (_a) {
            var uri = _a[0], doc = _a[1];
            return ({
                path: vscode_uri_1.URI.parse(uri).fsPath,
                doc: doc,
            });
        });
    };
    FileCache.prototype.getDocuments = function () {
        return this.cacheSummary;
    };
    FileCache.prototype.getDocument = function (uri) {
        return this.cache.get(uri.toString());
    };
    FileCache.prototype.define = function (name) {
        return this.__definitions.get(name);
    };
    Object.defineProperty(FileCache.prototype, "definitions", {
        get: function () {
            return Array.from(this.__definitions.values());
        },
        enumerable: false,
        configurable: true
    });
    FileCache.prototype.setDB = function (parser) {
        var _this = this;
        this.__definitions.clear();
        [
            { type: 'enum', v: parser.enums },
            { type: 'class', v: parser.classes },
            { type: 'client', v: parser.clients },
            { type: 'functions', v: parser.functions },
        ].forEach(function (_a) {
            var type = _a.type, v = _a.v;
            v.forEach(function (e) {
                var doc = _this.getDocument(vscode_uri_1.URI.file(e.name.source_file));
                if (!doc) {
                    return;
                }
                var start = (0, ast_1.getPositionFromIndex)(doc, e.name.start);
                var end = (0, ast_1.getPositionFromIndex)(doc, e.name.end);
                if (type === 'functions') {
                    var func = e;
                    var fromArgType = function (arg) {
                        if (arg.arg_type === 'positional') {
                            return "".concat(arg.type);
                        }
                        else {
                            return arg.values.map(function (v) { return "".concat(v.name.value, ": ").concat(v.type); }).join(', ');
                        }
                    };
                    _this.__definitions.set(e.name.value, {
                        name: e.name.value,
                        range: { start: start, end: end },
                        uri: vscode_uri_1.URI.file(e.name.source_file),
                        type: 'function',
                        input: fromArgType(func.input),
                        output: fromArgType(func.output),
                    });
                }
                else {
                    _this.__definitions.set(e.name.value, {
                        name: e.name.value,
                        range: { start: start, end: end },
                        uri: vscode_uri_1.URI.file(e.name.source_file),
                        type: type,
                    });
                }
            });
        });
    };
    return FileCache;
}());
exports.FileCache = FileCache;
