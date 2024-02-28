"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var hooks_1 = require("./hooks");
var data_table_1 = require("./TestResults/data-table");
var columns_1 = require("./TestResults/columns");
var react_1 = require("@vscode/webview-ui-toolkit/react");
var react_2 = require("react");
var scroll_area_1 = require("@/components/ui/scroll-area");
var button_1 = require("@/components/ui/button");
var lucide_react_1 = require("lucide-react");
var AnsiText_1 = require("@/utils/AnsiText");
var papaparse_1 = require("papaparse");
var vscode_1 = require("@/utils/vscode");
var TestResultPanel = function () {
    var _a = (0, hooks_1.useSelections)(), test_results = _a.test_results, test_result_url = _a.test_result_url, test_result_exit_status = _a.test_result_exit_status;
    var _b = (0, react_2.useState)('summary'), selected = _b[0], setSelection = _b[1];
    if (!test_results) {
        return (<>
        <div className="flex flex-col items-center justify-center">
          <div className="flex flex-col items-center justify-center space-y-2">
            <div className="text-base font-semibold text-vscode-descriptionForeground">
              No test results for this function
            </div>
            <div className="text-sm font-light">Run tests to see results</div>
          </div>
        </div>
      </>);
    }
    return (<>
      {test_result_url && (<div className="flex flex-row items-center justify-center w-full bg-vscode-menu-background">
          <react_1.VSCodeLink href={test_result_url.url}>
            <div className="flex flex-row gap-1 py-0.5 text-xs">
              {test_result_url.text} <lucide_react_1.ExternalLink className="w-4 h-4"/>
            </div>
          </react_1.VSCodeLink>
        </div>)}
      <div className="relative flex flex-col w-full h-full">
        <react_1.VSCodePanels activeid={"test-".concat(selected)} onChange={function (e) {
            var _a, _b;
            var selected = (_b = (_a = e.target) === null || _a === void 0 ? void 0 : _a.activetab) === null || _b === void 0 ? void 0 : _b.id;
            if (selected && selected.startsWith("test-")) {
                setSelection(selected.split('-', 2)[1]);
            }
        }} className="h-full">
          <react_1.VSCodePanelTab id={"test-summary"}>Summary</react_1.VSCodePanelTab>
          <react_1.VSCodePanelView id={"view-summary"} className="">
            <div className="flex flex-col w-full gap-y-1">
              {test_result_exit_status === 'ERROR' && (<div className="flex flex-row items-center justify-center w-full h-full space-x-2">
                  <div className="flex flex-col items-center justify-center space-y-2">
                    <div className="flex flex-row items-center gap-x-2">
                      <lucide_react_1.AlertTriangle className="w-4 h-4 text-vscode-editorWarning-foreground"/>
                      <div className="text-xs text-vscode-editorWarning-foreground">Test exited with an error</div>
                    </div>

                    <div className="text-xs font-light">Check the output tab for more details</div>
                  </div>
                </div>)}
              <data_table_1.DataTable columns={columns_1.columns} data={test_results}/>
            </div>
          </react_1.VSCodePanelView>
          <react_1.VSCodePanelTab id={"test-logs"}>
            <div className="flex flex-row gap-1">
              {test_result_exit_status === 'RUNNING' && <react_1.VSCodeProgressRing className="h-4"/>}
              {test_result_exit_status === 'ERROR' && (<lucide_react_1.AlertTriangle className="w-4 h-4 text-vscode-editorWarning-foreground"/>)}{' '}
              Output
            </div>
          </react_1.VSCodePanelTab>
          <react_1.VSCodePanelView id={"view-logs"}>
            <scroll_area_1.ScrollArea type="always" className="flex w-full h-full pr-3">
              <TestLogPanel />
            </scroll_area_1.ScrollArea>
          </react_1.VSCodePanelView>
        </react_1.VSCodePanels>
        {test_result_exit_status === 'COMPLETED' || test_result_exit_status === 'ERROR' ? (<div className="absolute right-0 z-20 top-1">
            <button_1.Button className="flex flex-row px-2 py-1 rounded-sm bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground w-fit h-fit whitespace-nowrap gap-x-1" onClick={function () {
                var test_csv = test_results.map(function (test) { return ({
                    function_name: test.functionName,
                    test_name: test.testName,
                    impl_name: test.implName,
                    input: test.input,
                    output_raw: test.output.raw,
                    output_parsed: test.output.parsed,
                    output_error: test.output.error,
                    status: test.status,
                    url: test.url,
                }); });
                vscode_1.vscode.postMessage({
                    command: 'downloadTestResults',
                    data: papaparse_1.default.unparse(test_csv),
                });
            }}>
              <lucide_react_1.Download className="w-4 h-4"/>
              <span className="pl-1 text-xs">CSV</span>
            </button_1.Button>
          </div>) : null}
      </div>
    </>);
};
var TestLogPanel = function () {
    var test_log = (0, hooks_1.useSelections)().test_log;
    return (<div className="h-full overflow-auto text-xs bg-vscode-terminal-background">
      {test_log ? (<AnsiText_1.default text={test_log} className="w-full break-all whitespace-break-spaces bg-inherit text-inherit"/>) : (<div className="flex flex-col items-center justify-center w-full h-full space-y-2">Waiting</div>)}
    </div>);
};
exports.default = TestResultPanel;
