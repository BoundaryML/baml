"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.getCwd = void 0;
var path_1 = __importDefault(require("path"));
var hasOwnProperty = function (object, property) {
    return Object.prototype.hasOwnProperty.call(object, property);
};
exports.getCwd = function (dir, scriptMode, scriptPath) {
    if (scriptMode) {
        if (!scriptPath) {
            console.error('Script mode must be used with a script name, e.g. `ts-node-dev -s <script.ts>`');
            process.exit();
        }
        if (dir) {
            console.error('Script mode cannot be combined with `--dir`');
            process.exit();
        }
        // Use node's own resolution behavior to ensure we follow symlinks.
        // scriptPath may omit file extension or point to a directory with or without package.json.
        // This happens before we are registered, so we tell node's resolver to consider ts, tsx, and jsx files.
        // In extremely rare cases, is is technically possible to resolve the wrong directory,
        // because we do not yet know preferTsExts, jsx, nor allowJs.
        // See also, justification why this will not happen in real-world situations:
        // https://github.com/TypeStrong/ts-node/pull/1009#issuecomment-613017081
        var exts = ['.js', '.jsx', '.ts', '.tsx'];
        var extsTemporarilyInstalled = [];
        for (var _i = 0, exts_1 = exts; _i < exts_1.length; _i++) {
            var ext = exts_1[_i];
            if (!hasOwnProperty(require.extensions, ext)) {
                // tslint:disable-line
                extsTemporarilyInstalled.push(ext);
                require.extensions[ext] = function () { }; // tslint:disable-line
            }
        }
        try {
            return path_1.default.dirname(require.resolve(scriptPath));
        }
        finally {
            for (var _a = 0, extsTemporarilyInstalled_1 = extsTemporarilyInstalled; _a < extsTemporarilyInstalled_1.length; _a++) {
                var ext = extsTemporarilyInstalled_1[_a];
                delete require.extensions[ext]; // tslint:disable-line
            }
        }
    }
    return dir;
};
