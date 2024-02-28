"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.KeywordCompletionProvider = void 0;
var vscode = require("vscode");
var keywords = [
    "@test_group",
    "@input",
    "@alias",
    "@description",
    "@skip",
    "@stringify",
    "@client",
    "@method",
    "@lang",
    "@provider",
];
var commitCharacters = [
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "_",
];
var KeywordCompletionProvider = /** @class */ (function () {
    function KeywordCompletionProvider() {
    }
    KeywordCompletionProvider.prototype.provideCompletionItems = function (document, position, token, context) {
        var line = document.lineAt(position).text;
        var prefix = line.slice(0, position.character);
        var match = prefix.match(/@(\w*)$/);
        if (match) {
            var userTyped_1 = match[1];
            var startPos = position.translate(0, -userTyped_1.length - 1); // -1 to account for "@"
            var endPos = position.translate(0, line.length - position.character);
            var replaceRange_1 = new vscode.Range(startPos, endPos);
            var completion = keywords
                .filter(function (keyword) { return keyword.startsWith("@".concat(userTyped_1)); })
                .map(function (keyword) {
                var item = new vscode.CompletionItem(keyword, vscode.CompletionItemKind.Keyword);
                // item.insertText = keyword.slice(1);
                item.range = replaceRange_1;
                item.filterText = "@";
                return item;
            });
            console.log(completion);
            return completion;
        }
    };
    return KeywordCompletionProvider;
}());
exports.KeywordCompletionProvider = KeywordCompletionProvider;
