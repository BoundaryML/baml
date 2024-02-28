"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var assert = require("assert");
// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
var vscode = require("vscode");
// import * as myExtension from '../../extension';
suite('Extension Test Suite', function () {
    vscode.window.showInformationMessage('Start all tests.');
    test('Sample test', function () {
        assert.strictEqual(-1, [1, 2, 3].indexOf(5));
        assert.strictEqual(-1, [1, 2, 3].indexOf(0));
    });
});
