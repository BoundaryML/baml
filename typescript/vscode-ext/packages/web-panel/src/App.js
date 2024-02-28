"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var react_1 = require("react");
require("./App.css");
require("allotment/dist/style.css");
var ASTProvider_1 = require("./shared/ASTProvider");
var FunctionPanel_1 = require("./shared/FunctionPanel");
var Selectors_1 = require("./shared/Selectors");
var react_2 = require("@vscode/webview-ui-toolkit/react");
var ErrorFallback_1 = require("./utils/ErrorFallback");
var separator_1 = require("./components/ui/separator");
var button_1 = require("./components/ui/button");
var hooks_1 = require("./shared/hooks");
var TestToggle = function () {
    var setSelection = (0, react_1.useContext)(ASTProvider_1.ASTContext).setSelection;
    var showTests = (0, hooks_1.useSelections)().showTests;
    return (<button_1.Button variant="outline" className="p-1 text-xs w-fit h-fit border-vscode-textSeparator-foreground" onClick={function () { return setSelection(undefined, undefined, undefined, undefined, !showTests); }}>
      {showTests ? 'Hide tests' : 'Show tests'}
    </button_1.Button>);
};
function App() {
    var _a = (0, react_1.useState)(true), selected = _a[0], setSelected = _a[1];
    return (<ErrorFallback_1.default>
      <ASTProvider_1.ASTProvider>
        <div className="absolute z-10 flex flex-col items-end gap-1 right-1 top-2 text-end">
          <TestToggle />
          <react_2.VSCodeLink href="https://docs.boundaryml.com">Docs</react_2.VSCodeLink>
        </div>
        <div className="flex flex-col gap-2 px-2 pb-4">
          <Selectors_1.FunctionSelector />
          <separator_1.Separator className="bg-vscode-textSeparator-foreground"/>
          <FunctionPanel_1.default />
        </div>
      </ASTProvider_1.ASTProvider>
    </ErrorFallback_1.default>);
}
exports.default = App;
