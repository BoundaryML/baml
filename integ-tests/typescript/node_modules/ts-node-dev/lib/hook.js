"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.makeHook = void 0;
var vm_1 = __importDefault(require("vm"));
exports.makeHook = function (cfg, wrapper, callback) {
    // Hook into Node's `require(...)`
    updateHooks();
    // Patch the vm module to watch files executed via one of these methods:
    if (cfg.vm) {
        patch(vm_1.default, 'createScript', 1);
        patch(vm_1.default, 'runInThisContext', 1);
        patch(vm_1.default, 'runInNewContext', 2);
        patch(vm_1.default, 'runInContext', 2);
    }
    /**
     * Patch the specified method to watch the file at the given argument
     * index.
     */
    function patch(obj, method, optionsArgIndex) {
        var orig = obj[method];
        if (!orig)
            return;
        obj[method] = function () {
            var opts = arguments[optionsArgIndex];
            var file = null;
            if (opts) {
                file = typeof opts == 'string' ? opts : opts.filename;
            }
            if (file)
                callback(file);
            return orig.apply(this, arguments);
        };
    }
    /**
     * (Re-)install hooks for all registered file extensions.
     */
    function updateHooks() {
        Object.keys(require.extensions).forEach(function (ext) {
            var fn = require.extensions[ext];
            if (typeof fn === 'function' && fn.name !== 'nodeDevHook') {
                require.extensions[ext] = createHook(fn);
            }
        });
    }
    /**
     * Returns a function that can be put into `require.extensions` in order to
     * invoke the callback when a module is required for the first time.
     */
    function createHook(handler) {
        return function nodeDevHook(module, filename) {
            if (module.parent === wrapper) {
                // If the main module is required conceal the wrapper
                module.id = '.';
                module.parent = null;
                process.mainModule = module;
            }
            if (!module.loaded)
                callback(module.filename);
            // Invoke the original handler
            handler(module, filename);
            // Make sure the module did not hijack the handler
            updateHooks();
        };
    }
};
