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
Object.defineProperty(exports, "__esModule", { value: true });
var childProcess = require('child_process');
var child_process_1 = require("child_process");
var resolve = require('resolve').sync;
var hook_1 = require("./hook");
var ipc = __importStar(require("./ipc"));
var resolveMain_1 = require("./resolveMain");
var cfg_1 = require("./cfg");
// const Module = require('module')
// Remove wrap.js from the argv array
process.argv.splice(1, 1);
// Resolve the location of the main script relative to cwd
var main = resolveMain_1.resolveMain(process.argv[1]);
var cfg = cfg_1.makeCfg(main, {});
if (process.env.TS_NODE_DEV === undefined) {
    process.env.TS_NODE_DEV = 'true';
}
if (process.env.NODE_DEV_PRELOAD) {
    require(process.env.NODE_DEV_PRELOAD);
}
// Listen SIGTERM and exit unless there is another listener
process.on('SIGTERM', function () {
    if (process.listeners('SIGTERM').length === 1)
        process.exit(0);
});
if (cfg.fork) {
    var oldFork_1 = child_process_1.fork;
    // Overwrite child_process.fork() so that we can hook into forked processes
    // too. We also need to relay messages about required files to the parent.
    var newFork = function (modulePath, args, options) {
        var child = oldFork_1(__filename, [modulePath].concat(args), options);
        ipc.relay(child);
        return child;
    };
    childProcess.fork = newFork;
}
// const lastRequired = null
// const origRequire = Module.prototype.require
// Module.prototype.require = function (requirePath) {
//   lastRequired = { path: requirePath, filename: this.filename }
//   return origRequire.apply(this, arguments)
// }
// Error handler that displays a notification and logs the stack to stderr:
var caught = false;
process.on('uncaughtException', function (err) {
    // NB: err can be null
    // Handle exception only once
    if (caught)
        return;
    caught = true;
    // If there's a custom uncaughtException handler expect it to terminate
    // the process.
    var hasCustomHandler = process.listeners('uncaughtException').length > 1;
    var isTsError = err && err.message && /TypeScript/.test(err.message);
    if (!hasCustomHandler && !isTsError) {
        console.error((err && err.stack) || err);
    }
    ipc.send({
        error: isTsError ? '' : (err && err.name) || 'Error',
        // lastRequired: lastRequired,
        message: err ? err.message : '',
        code: err && err.code,
        willTerminate: hasCustomHandler,
    });
});
// Hook into require() and notify the parent process about required files
hook_1.makeHook(cfg, module, function (file) {
    ipc.send({ required: file });
});
// Check if a module is registered for this extension
// const ext = path.extname(main).slice(1)
// const mod = cfg.extensions[ext]
// // Support extensions where 'require' returns a function that accepts options
// if (typeof mod == 'object' && mod.name) {
//   const fn = require(resolve(mod.name, { basedir: path.dirname(main) }))
//   if (typeof fn == 'function' && mod.options) {
//     // require returned a function, call it with options
//     fn(mod.options)
//   }
// } else if (typeof mod == 'string') {
//   require(resolve(mod, { basedir: path.dirname(main) }))
// }
// Execute the wrapped script
require(main);
