"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var baml_schema_wasm_1 = require("@gloo-ai/baml-schema-wasm");
function callWasm() {
    var res = baml_schema_wasm_1.default.lint("test");
    console.log("res", res);
}
console.log("calling wasm");
callWasm();
