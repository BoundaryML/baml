"use strict";
// Based off Gloo-AI common utils
Object.defineProperty(exports, "__esModule", { value: true });
exports.parseGlooObject = void 0;
var parseGlooObject = function (obj) {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    try {
        // Create a mock JSON string with the value and parse it
        // eslint-disable-next-line @typescript-eslint/restrict-template-expressions
        var mockJson = "{\"tempKey\": ".concat(obj.value, "}");
        var parsedMock = JSON.parse(mockJson);
        // Extract the value from the mock JSON
        // eslint-disable-next-line @typescript-eslint/no-unsafe-return, @typescript-eslint/no-unsafe-member-access
        return parsedMock.tempKey;
    }
    catch (e) {
        // If parsing fails, return the original value
        // eslint-disable-next-line @typescript-eslint/no-unsafe-return
        return obj.value;
    }
};
exports.parseGlooObject = parseGlooObject;
