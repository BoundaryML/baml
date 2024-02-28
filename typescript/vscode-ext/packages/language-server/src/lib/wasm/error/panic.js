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
exports.getWasmError = exports.isWasmPanic = void 0;
var WasmPanicRegistry_1 = require("./WasmPanicRegistry");
/**
 * Returns true if the given error is a Wasm panic.
 */
function isWasmPanic(error) {
    return error.name === 'RuntimeError';
}
exports.isWasmPanic = isWasmPanic;
/**
 * Set up a global registry for Wasm panics.
 * This allows us to retrieve the panic message from the Wasm panic hook,
 * which is not possible otherwise.
 */
var globalWithWasm = globalThis;
// Only do this once.
globalWithWasm.PRISMA_WASM_PANIC_REGISTRY = new WasmPanicRegistry_1.WasmPanicRegistry();
function getWasmError(error) {
    var message = globalWithWasm.PRISMA_WASM_PANIC_REGISTRY.get();
    var stack = __spreadArray([message], (error.stack || 'NO_BACKTRACE').split('\n').slice(1), true).join('\n');
    return { message: message, stack: stack };
}
exports.getWasmError = getWasmError;
