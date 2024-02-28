"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.getPositionFromIndex = exports.getSymbolBeforePosition = exports.getWordAtPosition = exports.isFirstInsideBlock = exports.getCurrentLine = exports.fullDocumentRange = void 0;
var constants_1 = require("../constants");
function fullDocumentRange(document) {
    var lastLineId = document.lineCount - 1;
    return {
        start: { line: 0, character: 0 },
        end: { line: lastLineId, character: constants_1.MAX_SAFE_VALUE_i32 },
    };
}
exports.fullDocumentRange = fullDocumentRange;
function getCurrentLine(document, line) {
    return document.getText({
        start: { line: line, character: 0 },
        end: { line: line, character: constants_1.MAX_SAFE_VALUE_i32 },
    });
}
exports.getCurrentLine = getCurrentLine;
function isFirstInsideBlock(position, currentLine) {
    if (currentLine.trim().length === 0) {
        return true;
    }
    var stringTilPosition = currentLine.slice(0, position.character);
    var matchArray = /\w+/.exec(stringTilPosition);
    if (!matchArray) {
        return true;
    }
    return (matchArray.length === 1 &&
        matchArray.index !== undefined &&
        stringTilPosition.length - matchArray.index - matchArray[0].length === 0);
}
exports.isFirstInsideBlock = isFirstInsideBlock;
function getWordAtPosition(document, position) {
    var currentLine = getCurrentLine(document, position.line);
    // search for the word's beginning and end
    var beginning = currentLine.slice(0, position.character + 1).search(/\S+$/);
    var end = currentLine.slice(position.character).search(/\W/);
    if (end < 0) {
        return '';
    }
    return currentLine.slice(beginning, end + position.character);
}
exports.getWordAtPosition = getWordAtPosition;
function getSymbolBeforePosition(document, position) {
    return document.getText({
        start: {
            line: position.line,
            character: position.character - 1,
        },
        end: { line: position.line, character: position.character },
    });
}
exports.getSymbolBeforePosition = getSymbolBeforePosition;
function getPositionFromIndex(document, index) {
    var line = 0;
    var character = 0;
    for (var i = 0; i < index; i++) {
        if (document.getText()[i] === '\n') {
            line++;
            character = 0;
        }
        else {
            character++;
        }
    }
    return { line: line, character: character };
}
exports.getPositionFromIndex = getPositionFromIndex;
