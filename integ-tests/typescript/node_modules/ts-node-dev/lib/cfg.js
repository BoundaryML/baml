"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.makeCfg = void 0;
var fs_1 = __importDefault(require("fs"));
var path_1 = __importDefault(require("path"));
function read(dir) {
    var f = path_1.default.resolve(dir, '.node-dev.json');
    return fs_1.default.existsSync(f) ? JSON.parse(fs_1.default.readFileSync(f, 'utf-8')) : null;
}
function resolvePath(unresolvedPath) {
    return path_1.default.resolve(process.cwd(), unresolvedPath);
}
exports.makeCfg = function (main, opts) {
    var dir = main ? path_1.default.dirname(main) : '.';
    var userDir = process.env.HOME || process.env.USERPROFILE;
    var c = read(dir) || read(process.cwd()) || (userDir && read(userDir)) || {};
    c.deps = parseInt(opts['deps-level'] || '') || 0;
    if (typeof c.depsLevel === 'number')
        c.deps = c.depsLevel;
    if (opts) {
        // Overwrite with CLI opts ...
        if (opts['deps'] || opts['all-deps'])
            c.deps = -1;
        if (opts.dedupe)
            c.dedupe = true;
        if (opts.respawn)
            c.respawn = true;
        if (opts.notify === false)
            c.notify = false;
        if (opts.clear || opts.cls)
            c.clear = true;
        c.fork = opts.fork;
    }
    var ignoreWatchItems = opts['ignore-watch']
        ? []
            .concat(opts['ignore-watch'])
            .map(function (_) { return _.trim(); })
        : [];
    var ignoreWatch = ignoreWatchItems.concat(c.ignore || []);
    opts.debug && console.log('Ignore watch:', ignoreWatch);
    var ignore = ignoreWatch.concat(ignoreWatch.map(resolvePath));
    return {
        vm: c.vm !== false,
        fork: c.fork !== false,
        notify: c.notify !== false,
        deps: c.deps,
        timestamp: c.timestamp || (c.timestamp !== false && 'HH:MM:ss'),
        clear: !!c.clear,
        dedupe: !!c.dedupe,
        ignore: ignore,
        respawn: c.respawn || false,
        debug: !!opts.debug,
        quiet: !!opts.quiet,
        extensions: c.extensions,
    };
};
