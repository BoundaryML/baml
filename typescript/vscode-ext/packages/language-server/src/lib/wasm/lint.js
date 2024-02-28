"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var _1 = require(".");
var internals_1 = require("./internals");
function lint(input, onError) {
    try {
        if (process.env.FORCE_PANIC_baml_SCHEMA) {
            (0, internals_1.handleFormatPanic)(function () {
                console.debug('Triggering a Rust panic...');
                _1.languageWasm.debug_panic();
            });
        }
        var result = _1.languageWasm.lint(JSON.stringify(input));
        var parsed = JSON.parse(result);
        // console.log(`lint result ${JSON.stringify(JSON.parse(result), null, 2)}`)
        return parsed;
    }
    catch (e) {
        var err = e;
        (0, internals_1.handleWasmError)(err, 'lint', onError);
        return {
            ok: false,
            diagnostics: [],
        };
    }
}
exports.default = lint;
