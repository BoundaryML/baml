"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.clientEventLogSchema = exports.getFullTestName = exports.TestStatus = void 0;
var TestStatus;
(function (TestStatus) {
    TestStatus["Compiling"] = "COMPILING";
    TestStatus["Queued"] = "QUEUED";
    TestStatus["Running"] = "RUNNING";
    TestStatus["Passed"] = "PASSED";
    TestStatus["Failed"] = "FAILED";
})(TestStatus || (exports.TestStatus = TestStatus = {}));
function getFullTestName(testName, impl, fnName) {
    return "test_".concat(testName, "[").concat(fnName, "-").concat(impl, "]");
}
exports.getFullTestName = getFullTestName;
var schemav2_1 = require("./schemav2");
Object.defineProperty(exports, "clientEventLogSchema", { enumerable: true, get: function () { return schemav2_1.clientEventLogSchema; } });
