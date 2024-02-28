"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var _1 = require(".");
var internals_1 = require("./internals");
function generate_test_file(input, onError) {
    console.log('running generate_test_file() from baml-schema-wasm');
    try {
        if (process.env.FORCE_PANIC_baml_SCHEMA) {
            (0, internals_1.handleFormatPanic)(function () {
                console.debug('Triggering a Rust panic...');
                _1.languageWasm.debug_panic();
            });
        }
        // console.log(`generate input ${JSON.stringify(input, null, 2)}`)
        var result = _1.languageWasm.generate_test_file(JSON.stringify(input));
        var parsed = JSON.parse(result);
        // console.log(`generate result ${JSON.stringify(JSON.parse(result), null, 2)}`)
        return parsed;
    }
    catch (e) {
        var err = e;
        (0, internals_1.handleWasmError)(err, 'lint', onError);
        return {
            status: 'error',
            message: err.message,
        };
    }
}
exports.default = generate_test_file;
