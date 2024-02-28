"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var WasmPanicRegistry_1 = require("./WasmPanicRegistry");
/**
 * Set up a global registry for Wasm panics.
 * This allows us to retrieve the panic message from the Wasm panic hook,
 * which is not possible otherwise.
 */
var globalWithWasm = globalThis;
globalWithWasm.PRISMA_WASM_PANIC_REGISTRY = new WasmPanicRegistry_1.WasmPanicRegistry();
