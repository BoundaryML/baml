"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var react_1 = require("react");
var anser_1 = require("anser");
var getLinks = function (text) {
    var txt = text.replace(/[^<>\s]+\.log\b/gm, function (str) { return "<a href=\"vscode://file/".concat(str, "\">").concat(str, "</a>"); });
    var urlRegex = /(<span class="ansi-(?:[^"]+)">)(https?:\/\/[^\s<]+)(<\/span>)/g;
    // Replace log files with links (lines with only *.log)
    return txt.replace(urlRegex, function (match, startTag, url, endTag) {
        // Replace the span with an anchor tag
        return "".concat(startTag, "<a href=\"").concat(url, "\" target=\"_blank\" rel=\"noopener noreferrer\">").concat(url, "</a>").concat(endTag);
    });
};
var AnsiText = function (_a) {
    var text = _a.text, className = _a.className;
    // use tailwind vscode classes in App.css with use_classes
    var html = anser_1.default.ansiToHtml(anser_1.default.escapeForHtml(text), { use_classes: true });
    return <pre className={className} dangerouslySetInnerHTML={{ __html: getLinks(html) }}/>;
};
exports.default = AnsiText;
