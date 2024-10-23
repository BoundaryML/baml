"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.get_checks = exports.all_succeeded = void 0;
function all_succeeded(checks) {
    return get_checks(checks).every(check => check.status === "succeeded");
}
exports.all_succeeded = all_succeeded;
function get_checks(checks) {
    return Object.values(checks);
}
exports.get_checks = get_checks;
