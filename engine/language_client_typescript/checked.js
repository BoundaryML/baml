"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
function all_succeeded(checks) {
    return Object.values(checks).every(value => value.result == "succeeded");
}
