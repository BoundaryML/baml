"use strict";
/// Content once a function has been selected.
Object.defineProperty(exports, "__esModule", { value: true });
var hooks_1 = require("./hooks");
var react_1 = require("@vscode/webview-ui-toolkit/react");
var TestCasePanel_1 = require("./TestCasePanel");
var ImplPanel_1 = require("./ImplPanel");
var react_2 = require("react");
var ASTProvider_1 = require("./ASTProvider");
var allotment_1 = require("allotment");
var TestResultOutcomes_1 = require("./TestResultOutcomes");
var scroll_area_1 = require("@/components/ui/scroll-area");
var clsx_1 = require("clsx");
var tooltip_1 = require("@/components/ui/tooltip");
var FunctionPanel = function () {
    var _a = (0, react_2.useContext)(ASTProvider_1.ASTContext), showTests = _a.selections.showTests, setSelection = _a.setSelection;
    var _b = (0, hooks_1.useSelections)(), func = _b.func, impl = _b.impl;
    if (!func)
        return <div className="flex flex-col">No function selected</div>;
    var test_results = (0, hooks_1.useSelections)().test_results;
    var results = test_results !== null && test_results !== void 0 ? test_results : [];
    return (<div className="flex flex-col w-full overflow-auto" style={{
            height: 'calc(100vh - 80px)',
        }}>
      <tooltip_1.TooltipProvider>
        {/* <Allotment vertical> */}
        <div className={(0, clsx_1.default)('w-full flex-shrink-0 flex-grow-0', {
            'basis-[60%]': showTests && results.length > 0,
            'basis-[100%]': !showTests,
            'basis-[85%]': showTests && !(results.length > 0),
        })}>
          <allotment_1.Allotment className="h-full">
            {impl && (<allotment_1.Allotment.Pane className="px-0" minSize={200}>
                <div className="relative h-full">
                  <scroll_area_1.ScrollArea type="always" className="flex w-full h-full pr-3">
                    <react_1.VSCodePanels activeid={"tab-".concat(func.name.value, "-").concat(impl.name.value)} onChange={function (e) {
                var _a, _b;
                var selected = (_b = (_a = e.target) === null || _a === void 0 ? void 0 : _a.activetab) === null || _b === void 0 ? void 0 : _b.id;
                if (selected && selected.startsWith("tab-".concat(func.name.value, "-"))) {
                    setSelection(undefined, undefined, selected.split('-', 3)[2], undefined, undefined);
                }
            }}>
                      {func.impls.map(function (impl) { return (<ImplPanel_1.default impl={impl} key={"".concat(func.name.value, "-").concat(impl.name.value)}/>); })}
                    </react_1.VSCodePanels>
                  </scroll_area_1.ScrollArea>
                </div>
              </allotment_1.Allotment.Pane>)}
            <allotment_1.Allotment.Pane className="pl-2 pr-0.5" minSize={200} visible={showTests}>
              <div className="w-full h-full overflow-auto">
                <scroll_area_1.ScrollArea type="always" className="flex w-full h-full pr-3">
                  <TestCasePanel_1.default func={func}/>
                </scroll_area_1.ScrollArea>
              </div>
            </allotment_1.Allotment.Pane>
          </allotment_1.Allotment>
        </div>
        <div className={(0, clsx_1.default)('py-2 border-t h-fit border-vscode-textSeparator-foreground', {
            flex: showTests,
            hidden: !showTests,
        })}>
          <div className="w-full h-full">
            <TestResultOutcomes_1.default />
          </div>
        </div>
      </tooltip_1.TooltipProvider>
    </div>);
};
exports.default = FunctionPanel;
