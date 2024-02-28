'use client';
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.columns = void 0;
var react_icons_1 = require("@radix-ui/react-icons");
var button_1 = require("@/components/ui/button");
var common_1 = require("@baml/common");
var react_1 = require("@vscode/webview-ui-toolkit/react");
var lucide_react_1 = require("lucide-react");
var react_2 = require("react");
var react18_json_view_1 = require("react18-json-view");
require("react18-json-view/src/style.css");
var schemaUtils_1 = require("../schemaUtils");
var hover_card_1 = require("@/components/ui/hover-card");
var Link_1 = require("../Link");
var switch_1 = require("@/components/ui/switch");
var label_1 = require("@/components/ui/label");
var TestStatusIcon = function (_a) {
    var _b;
    var testStatus = _a.testStatus, children = _a.children;
    return (<div className="text-vscode-descriptionForeground">
      {(_b = {},
            _b[common_1.TestStatus.Compiling] = 'Compiling',
            _b[common_1.TestStatus.Queued] = 'Queued',
            _b[common_1.TestStatus.Running] = <react_1.VSCodeProgressRing className="h-4"/>,
            _b[common_1.TestStatus.Passed] = (<div className="flex flex-row items-center gap-1">
              <div className="text-vscode-testing-iconPassed">Passed</div>
              {children}
            </div>),
            _b[common_1.TestStatus.Failed] = (<div className="flex flex-row items-center gap-1">
              <div className="text-vscode-testing-iconFailed">Failed</div>
              {children}
            </div>),
            _b)[testStatus]}
    </div>);
};
exports.columns = [
    {
        header: function (_a) {
            var column = _a.column;
            return (<button_1.Button variant="ghost" className="py-1 hover:bg-vscode-list-hoverBackground hover:text-vscode-list-hoverForeground h-fit" onClick={function () { return column.toggleSorting(column.getIsSorted() === 'asc'); }}>
          Test Case
          <react_icons_1.CaretSortIcon className="w-4 h-4 ml-2"/>
        </button_1.Button>);
        },
        cell: function (_a) {
            var getValue = _a.getValue, row = _a.row, cell = _a.cell;
            return (<hover_card_1.HoverCard openDelay={50} closeDelay={0}>
          <hover_card_1.HoverCardTrigger>
            <div className="flex flex-row items-center gap-1 text-center w-fit">
              <div className="underline">
                {row.original.span ? (<Link_1.default item={row.original.span} display={row.original.testName}>
                    {row.original.testName}
                  </Link_1.default>) : (<>{row.original.testName}</>)}
              </div>
              <div className="text-xs text-vscode-descriptionForeground">({row.original.implName})</div>
            </div>
          </hover_card_1.HoverCardTrigger>
          <hover_card_1.HoverCardContent side="top" sideOffset={6} className="px-1 min-w-[400px] py-1 break-all border-0 border-none bg-vscode-input-background text-vscode-input-foreground overflow-y-scroll max-h-[500px] text-xs">
            <react18_json_view_1.default enableClipboard={false} className="bg-[#1E1E1E] " theme="a11y" collapseStringsAfterLength={600} src={(0, schemaUtils_1.parseGlooObject)({
                    value: row.original.input,
                })}/>
          </hover_card_1.HoverCardContent>
        </hover_card_1.HoverCard>);
        },
        accessorFn: function (row) { return "".concat(row.testName, "-").concat(row.implName); },
        id: 'testName-implName',
    },
    {
        id: 'status',
        accessorFn: function (row) { return ({
            status: row.status,
            error: row.output.error,
            render: row.output.parsed,
            raw: row.output.raw,
            url: row.url,
        }); },
        cell: function (_a) {
            var getValue = _a.getValue;
            var val = getValue();
            var _b = (0, react_2.useState)(true), showJson = _b[0], setShowJson = _b[1];
            return (<div className="flex flex-col w-full p-0 text-xs">
          <div className="flex flex-row justify-between gap-x-1">
            <TestStatusIcon testStatus={val.status}>
              {val.url && (<react_1.VSCodeLink href={val.url}>
                  <lucide_react_1.ExternalLink className="w-4 h-4"/>
                </react_1.VSCodeLink>)}
            </TestStatusIcon>
            {val.render ? (<div className="">
                <div className="flex items-center space-x-2">
                  <label_1.Label htmlFor="output" className="text-xs font-light text-vscode-descriptionForeground opacity-80">
                    Show Raw Output
                  </label_1.Label>
                  <switch_1.Switch id="output" className="data-[state=checked]:bg-vscode-button-background data-[state=unchecked]:bg-vscode-input-background scale-75" onCheckedChange={function (e) { return setShowJson(!e); }} checked={!showJson}/>
                </div>
              </div>) : null}
          </div>

          {val.error && (<pre className="break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5">
              {pretty_error(val.error)}
            </pre>)}
          {val.render && (<pre className="break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5 relative bg-[#1E1E1E] text-white/90">
              {!showJson ? (val.raw) : (<react18_json_view_1.default enableClipboard={false} className="bg-[#1E1E1E]" theme="a11y" collapseStringsAfterLength={600} src={(0, schemaUtils_1.parseGlooObject)({
                            value: pretty_stringify(val.render),
                        })}/>)}
            </pre>)}
        </div>);
        },
        header: 'Status',
    },
];
var pretty_error = function (obj) {
    try {
        var err = JSON.parse(obj);
        return err.error;
    }
    catch (e) {
        return obj;
    }
};
var pretty_stringify = function (obj) {
    try {
        return JSON.stringify(JSON.parse(obj), null, 2);
    }
    catch (e) {
        return obj;
    }
};
