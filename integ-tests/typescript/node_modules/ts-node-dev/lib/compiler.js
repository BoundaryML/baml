"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    Object.defineProperty(o, k2, { enumerable: true, get: function() { return m[k]; } });
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || function (mod) {
    if (mod && mod.__esModule) return mod;
    var result = {};
    if (mod != null) for (var k in mod) if (k !== "default" && Object.hasOwnProperty.call(mod, k)) __createBinding(result, mod, k);
    __setModuleDefault(result, mod);
    return result;
};
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.makeCompiler = void 0;
var tsNode = __importStar(require("ts-node"));
var fs_1 = __importDefault(require("fs"));
var path_1 = __importDefault(require("path"));
var os_1 = __importDefault(require("os"));
var mkdirp_1 = __importDefault(require("mkdirp"));
var rimraf_1 = __importDefault(require("rimraf"));
var tsconfig_1 = require("tsconfig");
var get_compiled_path_1 = require("./get-compiled-path");
var get_cwd_1 = require("./get-cwd");
var fixPath = function (p) { return p.replace(/\\/g, '/').replace(/\$/g, '$$$$'); };
var sourceMapSupportPath = require.resolve('source-map-support');
var compileExtensions = ['.ts', '.tsx'];
var cwd = process.cwd();
var compilationInstanceStamp = Math.random().toString().slice(2);
var originalJsHandler = require.extensions['.js'];
var parse = function (value) {
    return typeof value === 'string' ? JSON.parse(value) : undefined;
};
function split(value) {
    return typeof value === 'string'
        ? value.split(/ *, */g).filter(function (v) { return v !== ''; })
        : undefined;
}
exports.makeCompiler = function (options, _a) {
    var log = _a.log, restart = _a.restart;
    var _errorCompileTimeout;
    var allowJs = false;
    var project = options['project'];
    var tsConfigPath = tsconfig_1.resolveSync(cwd, typeof project === 'string' ? project : undefined) || '';
    var compiledPathsHash = {};
    var tmpDir = options['cache-directory']
        ? path_1.default.resolve(options['cache-directory'])
        : fs_1.default.mkdtempSync(path_1.default.join(os_1.default.tmpdir(), '.ts-node'));
    var writeChildHookFile = function (options) {
        var compileTimeout = parseInt(options['compile-timeout']);
        var getIgnoreVal = function (ignore) {
            var ignoreVal = !ignore || ignore === 'false'
                ? 'false'
                : '[' +
                    ignore
                        .split(/,/)
                        .map(function (i) { return i.trim(); })
                        .map(function (ignore) { return 'new RegExp("' + ignore + '")'; })
                        .join(', ') +
                    ']';
            return ignoreVal;
        };
        var varDecl = function (name, value) { return "var " + name + " = '" + value + "'"; };
        var replacements = [
            compileTimeout ? ['10000', compileTimeout.toString()] : null,
            allowJs ? ['allowJs = false', 'allowJs = true'] : null,
            options['prefer-ts-exts']
                ? ['preferTs = false', 'preferTs = true']
                : null,
            options['exec-check'] ? ['execCheck = false', 'execCheck = true'] : null,
            options['exit-child'] ? ['exitChild = false', 'exitChild = true'] : null,
            options['ignore'] !== undefined
                ? [
                    'var ignore = [/node_modules/]',
                    'var ignore = ' + getIgnoreVal(options['ignore']),
                ]
                : null,
            [
                varDecl('compilationId', ''),
                varDecl('compilationId', getCompilationId()),
            ],
            [varDecl('compiledDir', ''), varDecl('compiledDir', getCompiledDir())],
            [
                './get-compiled-path',
                fixPath(path_1.default.join(__dirname, 'get-compiled-path')),
            ],
            [
                varDecl('readyFile', ''),
                varDecl('readyFile', getCompilerReadyFilePath()),
            ],
            [
                varDecl('sourceMapSupportPath', ''),
                varDecl('sourceMapSupportPath', fixPath(sourceMapSupportPath)),
            ],
            [
                varDecl('libPath', ''),
                varDecl('libPath', __dirname.replace(/\\/g, '\\\\')),
            ],
            ['__dirname', '"' + fixPath(__dirname) + '"'],
        ]
            .filter(function (_) { return !!_; })
            .map(function (_) { return _; });
        var fileText = fs_1.default.readFileSync(path_1.default.join(__dirname, 'child-require-hook.js'), 'utf-8');
        var fileData = replacements.reduce(function (text, _a) {
            var what = _a[0], to = _a[1];
            return text.replace(what, to);
        }, fileText);
        fs_1.default.writeFileSync(getChildHookPath(), fileData);
    };
    var init = function () {
        registerTsNode();
        /* clean up compiled on each new init*/
        rimraf_1.default.sync(getCompiledDir());
        createCompiledDir();
        // check if `allowJs` compiler option enable
        // (.js handler was changed while ts-node registration)
        allowJs = require.extensions['.js'] !== originalJsHandler;
        if (allowJs) {
            compileExtensions.push('.js', '.jsx');
        }
        writeChildHookFile(options);
    };
    var getCompilationId = function () {
        return compilationInstanceStamp;
    };
    var createCompiledDir = function () {
        var compiledDir = getCompiledDir();
        if (!fs_1.default.existsSync(compiledDir)) {
            mkdirp_1.default.sync(getCompiledDir());
        }
    };
    var getCompiledDir = function () {
        return path_1.default.join(tmpDir, 'compiled').replace(/\\/g, '/');
    };
    var getCompileReqFilePath = function () {
        return path_1.default.join(getCompiledDir(), getCompilationId() + '.req');
    };
    var getCompilerReadyFilePath = function () {
        return path_1.default
            .join(os_1.default.tmpdir(), 'ts-node-dev-ready-' + compilationInstanceStamp)
            .replace(/\\/g, '/');
    };
    var getChildHookPath = function () {
        return path_1.default
            .join(os_1.default.tmpdir(), 'ts-node-dev-hook-' + compilationInstanceStamp + '.js')
            .replace(/\\/g, '/');
    };
    var writeReadyFile = function () {
        fs_1.default.writeFileSync(getCompilerReadyFilePath(), '');
    };
    var clearErrorCompile = function () {
        clearTimeout(_errorCompileTimeout);
    };
    var registerTsNode = function () {
        Object.keys(compiledPathsHash).forEach(function (key) {
            delete compiledPathsHash[key];
        });
        ['.js', '.jsx', '.ts', '.tsx'].forEach(function (ext) {
            require.extensions[ext] = originalJsHandler;
        });
        var scriptPath = options._.length
            ? path_1.default.resolve(cwd, options._[0])
            : undefined;
        tsNode.register({
            // --dir does not work (it gives a boolean only) so we only check for script-mode
            dir: get_cwd_1.getCwd(options['dir'], options['script-mode'], scriptPath),
            scope: options['scope'],
            scopeDir: options['scopeDir'],
            emit: options['emit'],
            files: options['files'],
            pretty: options['pretty'],
            transpileOnly: options['transpile-only'],
            ignore: options['ignore'] ? split(options['ignore']) : undefined,
            preferTsExts: options['prefer-ts-exts'],
            logError: options['log-error'],
            project: options['project'],
            skipProject: options['skip-project'],
            transpiler: options['transpiler'],
            skipIgnore: options['skip-ignore'],
            compiler: options['compiler'],
            compilerHost: options['compiler-host'],
            ignoreDiagnostics: options['ignore-diagnostics']
                ? split(options['ignore-diagnostics'])
                : undefined,
            compilerOptions: parse(options['compiler-options']),
        });
    };
    var compiler = {
        tsConfigPath: tsConfigPath,
        init: init,
        getCompileReqFilePath: getCompileReqFilePath,
        getChildHookPath: getChildHookPath,
        writeReadyFile: writeReadyFile,
        clearErrorCompile: clearErrorCompile,
        compileChanged: function (fileName) {
            var ext = path_1.default.extname(fileName);
            if (compileExtensions.indexOf(ext) < 0)
                return;
            try {
                var code = fs_1.default.readFileSync(fileName, 'utf-8');
                compiler.compile({
                    code: code,
                    compile: fileName,
                    compiledPath: get_compiled_path_1.getCompiledPath(code, fileName, getCompiledDir()),
                });
            }
            catch (e) {
                console.error(e);
            }
        },
        compile: function (params) {
            var fileName = params.compile;
            var code = fs_1.default.readFileSync(fileName, 'utf-8');
            var compiledPath = params.compiledPath;
            // Prevent occasional duplicate compilation requests
            if (compiledPathsHash[compiledPath]) {
                return;
            }
            compiledPathsHash[compiledPath] = true;
            function writeCompiled(code, fileName) {
                fs_1.default.writeFile(compiledPath, code, function (err) {
                    err && log.error(err);
                    fs_1.default.writeFile(compiledPath + '.done', '', function (err) {
                        err && log.error(err);
                    });
                });
            }
            if (fs_1.default.existsSync(compiledPath)) {
                return;
            }
            var starTime = new Date().getTime();
            var m = {
                _compile: writeCompiled,
            };
            var _compile = function () {
                var ext = path_1.default.extname(fileName);
                var extHandler = require.extensions[ext];
                extHandler(m, fileName);
                log.debug(fileName, 'compiled in', new Date().getTime() - starTime, 'ms');
            };
            try {
                _compile();
            }
            catch (e) {
                console.error('Compilation error in', fileName);
                var errorCode = 'throw ' +
                    'new Error(' +
                    JSON.stringify(e.message) +
                    ')' +
                    ';';
                writeCompiled(errorCode);
                // reinitialize ts-node compilation to clean up state after error
                // without timeout in causes cases error not be printed out
                setTimeout(function () {
                    registerTsNode();
                }, 0);
                if (!options['error-recompile']) {
                    return;
                }
                var timeoutMs_1 = parseInt(process.env.TS_NODE_DEV_ERROR_RECOMPILE_TIMEOUT || '0') ||
                    5000;
                var errorHandler_1 = function () {
                    clearTimeout(_errorCompileTimeout);
                    _errorCompileTimeout = setTimeout(function () {
                        try {
                            _compile();
                            restart(fileName);
                        }
                        catch (e) {
                            registerTsNode();
                            errorHandler_1();
                        }
                    }, timeoutMs_1);
                };
                errorHandler_1();
            }
        },
    };
    return compiler;
};
