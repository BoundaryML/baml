"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.WasmPanicRegistry = void 0;
var WasmPanicRegistry = /** @class */ (function () {
    function WasmPanicRegistry() {
        this.message = '';
    }
    WasmPanicRegistry.prototype.get = function () {
        return "".concat(this.message);
    };
    // Don't use this method directly, it's only used by the Wasm panic hook in @prisma/prisma-schema-wasm.
    WasmPanicRegistry.prototype.set_message = function (value) {
        this.message = "RuntimeError: ".concat(value);
    };
    return WasmPanicRegistry;
}());
exports.WasmPanicRegistry = WasmPanicRegistry;
