"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TestCaseSelector = exports.FunctionSelector = void 0;
var react_1 = require("@vscode/webview-ui-toolkit/react");
var hooks_1 = require("./hooks");
var react_2 = require("react");
var ASTProvider_1 = require("./ASTProvider");
var Link_1 = require("./Link");
var TypeComponent_1 = require("./TypeComponent");
var ProjectPanel_1 = require("./ProjectPanel");
var FunctionSelector = function () {
    var _a;
    var _b = (0, react_2.useContext)(ASTProvider_1.ASTContext), projects = _b.projects, selectedProjectId = _b.selectedProjectId, functions = _b.db.functions, setSelection = _b.setSelection;
    var func = (0, hooks_1.useSelections)().func;
    var function_names = functions.map(function (func) { return func.name.value; });
    return (<div className="flex flex-col items-start gap-1">
      <div className="flex flex-row items-center gap-1">
        <ProjectPanel_1.ProjectToggle />

        <span className="font-light">Function</span>
        <react_1.VSCodeDropdown value={(_a = func === null || func === void 0 ? void 0 : func.name.value) !== null && _a !== void 0 ? _a : '<not-picked>'} onChange={function (event) {
            return setSelection(undefined, event.currentTarget.value, undefined, undefined, undefined);
        }}>
          {function_names.map(function (func) { return (<react_1.VSCodeOption key={func} value={func}>
              {func}
            </react_1.VSCodeOption>); })}
        </react_1.VSCodeDropdown>
      </div>
      {func && (<div className="flex flex-row items-center gap-0 text-xs">
          <Link_1.default item={func.name}/>(
          {func.input.arg_type === 'positional' ? (<div className="flex flex-row gap-1">
              arg: <TypeComponent_1.default typeString={func.input.type}/>
            </div>) : (<div className="flex flex-row gap-1">
              {func.input.values.map(function (v) { return (<div key={v.name.value}>
                  {v.name.value}: <TypeComponent_1.default typeString={v.type}/>,
                </div>); })}
            </div>)}
          ) {'->'} {func.output.arg_type === 'positional' && <TypeComponent_1.default typeString={func.output.type}/>}
        </div>)}
    </div>);
};
exports.FunctionSelector = FunctionSelector;
var TestCaseSelector = function () {
    var _a, _b;
    var PLACEHOLDER = '<new>';
    var setSelection = (0, react_2.useContext)(ASTProvider_1.ASTContext).setSelection;
    var _c = (0, hooks_1.useSelections)(), func = _c.func, _d = _c.test_case, _e = _d === void 0 ? {} : _d, name = _e.name;
    var test_cases = (_a = func === null || func === void 0 ? void 0 : func.test_cases.map(function (cases) { return cases.name.value; })) !== null && _a !== void 0 ? _a : [];
    if (!func)
        return null;
    return (<>
      <react_1.VSCodeDropdown value={(_b = name === null || name === void 0 ? void 0 : name.value) !== null && _b !== void 0 ? _b : PLACEHOLDER} onChange={function (event) {
            var value = event.currentTarget.value;
            setSelection(undefined, undefined, undefined, value, undefined);
        }}>
        {test_cases.map(function (cases, index) { return (<react_1.VSCodeOption key={index} value={cases}>
            {cases}
          </react_1.VSCodeOption>); })}
        <react_1.VSCodeOption value={PLACEHOLDER}>{PLACEHOLDER}</react_1.VSCodeOption>
      </react_1.VSCodeDropdown>
      {name && <Link_1.default item={name} display="Open File"/>}
    </>);
};
exports.TestCaseSelector = TestCaseSelector;
