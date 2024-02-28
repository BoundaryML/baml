"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.handleFormatPanic = exports.handleWasmError = exports.getCliVersion = exports.getEnginesVersion = exports.getVersion = void 0;
var panic_1 = require("./error/panic");
var packageJson = require('../../../../package.json'); // eslint-disable-line
var _1 = require(".");
/* eslint-disable @typescript-eslint/no-unsafe-member-access,@typescript-eslint/no-unsafe-return */
/**
 * Lookup version. This is the version of the the generated language-wasm package.
 * Matches the version of the baml cli.
 */
function getVersion() {
    return _1.languageWasm.version();
}
exports.getVersion = getVersion;
/**
 * Gets Engines Version from package.json, dependencies, `@Baml/language-wasm`
 * @returns Something like `2.26.0-23.9b816b3aa13cc270074f172f30d6eda8a8ce867d`
 */
function getEnginesVersion() {
    return packageJson.dependencies['@Baml/language-wasm'];
}
exports.getEnginesVersion = getEnginesVersion;
/**
 * Gets CLI Version from package.json, Baml, cliVersion
 * @returns Something like `2.27.0-dev.50`
 */
function getCliVersion() {
    return _1.languageWasm.version();
}
exports.getCliVersion = getCliVersion;
function handleWasmError(e, cmd, onError) {
    var getErrorMessage = function () {
        if ((0, panic_1.isWasmPanic)(e)) {
            var _a = (0, panic_1.getWasmError)(e), message_1 = _a.message, stack_1 = _a.stack;
            var msg_1 = "language-wasm errored when invoking ".concat(cmd, ". It resulted in a Wasm panic.\n").concat(message_1);
            return { message: msg_1, isPanic: true, stack: stack_1 };
        }
        var msg = "language-wasm errored when invoking ".concat(cmd, ".\n").concat(e.message);
        return { message: msg, isPanic: false, stack: e.stack };
    };
    var _a = getErrorMessage(), message = _a.message, isPanic = _a.isPanic, stack = _a.stack;
    if (isPanic) {
        console.warn("language-wasm errored (panic) with: ".concat(message, "\n\n").concat(stack));
    }
    else {
        console.warn("language-wasm errored with: ".concat(message, "\n\n").concat(stack));
    }
    if (onError) {
        onError(
        // Note: VS Code strips newline characters from the message
        "language-wasm errored with: -- ".concat(message, " -- For the full output check the \"Baml Language Server\" output. In the menu, click \"View\", then Output and select \"Baml Language Server\" in the drop-down menu."));
    }
}
exports.handleWasmError = handleWasmError;
function handleFormatPanic(tryCb) {
    try {
        return tryCb();
    }
    catch (e) {
        throw (0, panic_1.getWasmError)(e);
    }
}
exports.handleFormatPanic = handleFormatPanic;
