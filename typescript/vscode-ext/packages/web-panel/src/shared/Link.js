"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var vscode_1 = require("@/utils/vscode");
var react_1 = require("@vscode/webview-ui-toolkit/react");
var Link = function (_a) {
    var item = _a.item, display = _a.display;
    return (<react_1.VSCodeLink className="text-vscode-list-activeSelectionForeground" onClick={function () {
            vscode_1.vscode.postMessage({ command: 'jumpToFile', data: item });
        }}>
    {display !== null && display !== void 0 ? display : item.value}
  </react_1.VSCodeLink>);
};
exports.default = Link;
