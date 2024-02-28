"use strict";
/// Content once a function has been selected.
var __assign = (this && this.__assign) || function () {
    __assign = Object.assign || function(t) {
        for (var s, i = 1, n = arguments.length; i < n; i++) {
            s = arguments[i];
            for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p))
                t[p] = s[p];
        }
        return t;
    };
    return __assign.apply(this, arguments);
};
var __rest = (this && this.__rest) || function (s, e) {
    var t = {};
    for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p) && e.indexOf(p) < 0)
        t[p] = s[p];
    if (s != null && typeof Object.getOwnPropertySymbols === "function")
        for (var i = 0, p = Object.getOwnPropertySymbols(s); i < p.length; i++) {
            if (e.indexOf(p[i]) < 0 && Object.prototype.propertyIsEnumerable.call(s, p[i]))
                t[p[i]] = s[p[i]];
        }
    return t;
};
Object.defineProperty(exports, "__esModule", { value: true });
var button_1 = require("@/components/ui/button");
var dialog_1 = require("@/components/ui/dialog");
var vscode_1 = require("@/utils/vscode");
var core_1 = require("@rjsf/core");
var utils_1 = require("@rjsf/utils");
var validator_ajv8_1 = require("@rjsf/validator-ajv8");
var react_1 = require("@vscode/webview-ui-toolkit/react");
var lucide_react_1 = require("lucide-react");
var react_2 = require("react");
var ASTProvider_1 = require("./ASTProvider");
var hooks_1 = require("./hooks");
var tooltip_1 = require("@/components/ui/tooltip");
var testSchema = {
    title: 'Test form',
    type: 'object',
    properties: {
        name: {
            type: 'string',
        },
        age: {
            type: 'number',
        },
    },
};
var _a = (0, core_1.getDefaultRegistry)().templates, BaseInputTemplate = _a.BaseInputTemplate, FieldTemplate = _a.FieldTemplate, ButtonTemplates = _a.ButtonTemplates;
function MyBaseInputTemplate(props) {
    var _a, _b;
    var id = props.id, name = props.name, // remove this from ...rest
    value = props.value, readonly = props.readonly, disabled = props.disabled, autofocus = props.autofocus, onBlur = props.onBlur, onFocus = props.onFocus, onChange = props.onChange, onChangeOverride = props.onChangeOverride, options = props.options, schema = props.schema, uiSchema = props.uiSchema, formContext = props.formContext, registry = props.registry, rawErrors = props.rawErrors, type = props.type, hideLabel = props.hideLabel, // remove this from ...rest
    hideError = props.hideError, // remove this from ...rest
    rest = __rest(props
    // Note: since React 15.2.0 we can't forward unknown element attributes, so we
    // exclude the "options" and "schema" ones here.
    , ["id", "name", "value", "readonly", "disabled", "autofocus", "onBlur", "onFocus", "onChange", "onChangeOverride", "options", "schema", "uiSchema", "formContext", "registry", "rawErrors", "type", "hideLabel", "hideError"]);
    // Note: since React 15.2.0 we can't forward unknown element attributes, so we
    // exclude the "options" and "schema" ones here.
    if (!id) {
        console.log('No id for', props);
        throw new Error("no id for props ".concat(JSON.stringify(props)));
    }
    var inputProps = __assign(__assign({}, rest), (0, utils_1.getInputProps)(schema, type, options));
    var inputValue;
    if (inputProps.type === 'number' || inputProps.type === 'integer') {
        inputValue = value || value === 0 ? value : '';
    }
    else {
        inputValue = value == null ? '' : value;
    }
    var _onChange = (0, react_2.useCallback)(function (_a) {
        var value = _a.target.value;
        return onChange(value === '' ? options.emptyValue : value);
    }, [onChange, options]);
    var _onBlur = (0, react_2.useCallback)(function (_a) {
        var value = _a.target.value;
        return onBlur(id, value);
    }, [onBlur, id]);
    var _onFocus = (0, react_2.useCallback)(function (_a) {
        var value = _a.target.value;
        return onFocus(id, value);
    }, [onFocus, id]);
    var length = Object.keys((_b = (_a = registry.rootSchema) === null || _a === void 0 ? void 0 : _a.definitions) !== null && _b !== void 0 ? _b : {}).length;
    var isSingleStringField = length === 0 && schema.type === 'string';
    var input = inputProps.type === 'number' || inputProps.type === 'integer' ? (<input id={id} name={id} className="max-w-[100px] rounded-sm bg-vscode-input-background text-vscode-input-foreground" readOnly={readonly} disabled={disabled} autoFocus={autofocus} value={inputValue} {...inputProps} list={schema.examples ? (0, utils_1.examplesId)(id) : undefined} onChange={onChangeOverride || _onChange} onBlur={_onBlur} onFocus={_onFocus} aria-describedby={(0, utils_1.ariaDescribedByIds)(id, !!schema.examples)}/>) : (<textarea id={id} name={id} rows={isSingleStringField ? 15 : 5} className="w-[90%] px-1 rounded-sm bg-vscode-input-background text-vscode-input-foreground" readOnly={readonly} disabled={disabled} autoFocus={autofocus} value={inputValue} {...inputProps} 
    // list={schema.examples ? examplesId(id) : undefined}
    onChange={onChangeOverride || _onChange} onBlur={_onBlur} onFocus={_onFocus} aria-describedby={(0, utils_1.ariaDescribedByIds)(id, !!schema.examples)}/>);
    return (<div className="flex flex-col w-full gap-y-1">
      {input}
      {Array.isArray(schema.examples) && (<datalist key={"datalist_".concat(id)} id={(0, utils_1.examplesId)(id)}>
          {schema.examples
                .concat(schema.default && !schema.examples.includes(schema.default) ? [schema.default] : [])
                .map(function (example) {
                return <option key={example} value={example}/>;
            })}
        </datalist>)}
    </div>);
}
// function MyFieldTemplate(props: FieldTemplateProps) {
//   return <FieldTemplate {...props} classNames="  gap-x-2" />
// }
function MyFieldTemplate(props) {
    var id = props.id, classNames = props.classNames, style = props.style, label = props.label, displayLabel = props.displayLabel, help = props.help, required = props.required, hidden = props.hidden, description = props.description, errors = props.errors, children = props.children;
    if (hidden) {
        return <div className="hidden">{children}</div>;
    }
    return (<div className={classNames + ' ml-2 w-full'} style={style}>
      <>
        {props.schema.type === 'boolean' ? null : (<label htmlFor={id} className="flex flex-row items-center gap-x-3">
            <div className={props.schema.type === 'object' ? ' font-bold text-sm' : ' text-xs'}>
              {label.split('-').at(-1)}
            </div>
            <div className={'text-vscode-textSeparator-foreground'}>
              {props.schema.type}
              {required ? '*' : null}
            </div>
          </label>)}
      </>

      {description}
      <div className="flex flex-row items-center w-full">{children}</div>
      {errors}
      {help}
    </div>);
}
function MyObjectTemplate(props) {
    return (<div className="w-full">
      {/* <div className="py-2">{props.title}</div> */}
      {props.description}
      <div className="flex flex-col w-full py-1 gap-y-2">
        {props.properties.map(function (element) { return (<div className="w-full property-wrapper text-vscode-input-foreground">{element.content}</div>); })}
      </div>
    </div>);
}
function AddButton(props) {
    var icon = props.icon, iconType = props.iconType, btnProps = __rest(props, ["icon", "iconType"]);
    return (<button_1.Button variant="ghost" size="icon" {...btnProps} className="flex flex-row items-center p-1 text-xs w-fit h-fit gap-x-2 hover:bg-vscode-descriptionForeground">
      <lucide_react_1.PlusIcon size={16}/> <div>Add item</div>
    </button_1.Button>);
}
function RemoveButton(props) {
    var icon = props.icon, iconType = props.iconType, btnProps = __rest(props, ["icon", "iconType"]);
    return (<div className="flex w-fit h-fit">
      <button_1.Button {...btnProps} size={'icon'} className="!flex flex-col !px-0 !py-0 bg-red-700 h-fit !max-w-[48px] ml-auto" style={{
            flex: 'none',
        }}>
        <lucide_react_1.X size={12}/>
      </button_1.Button>
    </div>);
}
function SubmitButton(props) {
    var icon = props.icon, iconType = props.iconType, btnProps = __rest(props, ["icon", "iconType"]);
    return (<div className="flex items-end justify-end w-full ml-auto h-fit">
      <button_1.Button {...btnProps} className="px-3 py-2 rounded-none bg-vscode-button-background text-vscode-button-foreground w-fit h-fit hover:bg-vscode-button-background hover:opacity-75" style={{
            flex: 'none',
        }}>
        Submit
      </button_1.Button>
    </div>);
}
function ArrayFieldItemTemplate(props) {
    var children = props.children, className = props.className;
    return (<div className="relative ">
      <div className={"".concat(className, " ml-0 py-1 text-xs text-vscode-descriptionForeground")}>{children}</div>
      {props.hasRemove && (<div className="absolute top-0 flex ml-auto right-4 w-fit h-fit">
          <button_1.Button onClick={props.onDropIndexClick(props.index)} disabled={props.disabled || props.readonly} size={'icon'} className="p-1 bg-transparent w-fit h-fit hover:bg-red-700" style={{
                flex: 'none',
            }}>
            <lucide_react_1.X size={12}/>
          </button_1.Button>
        </div>)}
    </div>);
}
function ArrayFieldTitleTemplate(props) {
    var title = props.title, idSchema = props.idSchema;
    var id = (0, utils_1.titleId)(idSchema);
    return null;
    // return (
    //   <div id={id} className="text-xs">
    //     {title}
    //   </div>
    // )
}
var uiSchema = {
    'ui:submitButtonOptions': {
        submitText: 'Save',
        props: {
            className: 'bg-vscode-button-background px-2',
        },
    },
    'ui:autocomplete': 'on',
    'ui:options': {
        removable: true,
    },
};
var TestCasePanel = function (_a) {
    var func = _a.func;
    var _b = (0, hooks_1.useSelections)(), impl = _b.impl, input_json_schema = _b.input_json_schema;
    var _c = (0, react_2.useState)(''), filter = _c[0], setFilter = _c[1];
    var test_cases = (0, react_2.useMemo)(function () {
        if (!filter)
            return func.test_cases;
        return func.test_cases.filter(function (test_case) { return test_case.name.value.includes(filter) || test_case.content.includes(filter); });
    }, [filter, func.test_cases]);
    var getTestParams = function (testCase) {
        if (func.input.arg_type === 'positional') {
            return {
                type: 'positional',
                value: testCase.content,
            };
        }
        else {
            // sort of a hack, means the test file was just initialized since input: null is the default
            if (testCase.content === 'null') {
                return {
                    type: 'named',
                    value: func.input.values.map(function (_a) {
                        var name = _a.name;
                        return ({
                            name: name.value,
                            value: null,
                        });
                    }),
                };
            }
            var parsed = JSON.parse(testCase.content);
            var contentMap_1 = new Map();
            if (typeof parsed === 'object') {
                contentMap_1 = new Map(Object.entries(parsed).map(function (_a) {
                    var k = _a[0], v = _a[1];
                    if (typeof v === 'string')
                        return [k, v];
                    return [k, JSON.stringify(v, null, 2)];
                }));
            }
            return {
                type: 'named',
                value: func.input.values.map(function (_a) {
                    var _b;
                    var name = _a.name;
                    return ({
                        name: name.value,
                        value: (_b = contentMap_1.get(name.value)) !== null && _b !== void 0 ? _b : null,
                    });
                }),
            };
        }
    };
    var _d = (0, react_2.useContext)(ASTProvider_1.ASTContext), root_path = _d.root_path, test_results = _d.test_results;
    return (<>
      <div className="flex flex-row justify-between gap-x-1">
        <react_1.VSCodeTextField placeholder="Search test cases" className="w-32 shrink" value={filter} onInput={function (e) {
            setFilter(e.currentTarget.value);
        }}/>
        {(test_results === null || test_results === void 0 ? void 0 : test_results.run_status) === 'RUNNING' ? (<react_1.VSCodeButton className="bg-vscode-statusBarItem-errorBackground" onClick={function () { return vscode_1.vscode.postMessage({ command: 'cancelTestRun' }); }}>
            Cancel
          </react_1.VSCodeButton>) : (<>
            <button_1.Button className="px-1 py-1 text-sm round ed-sm h-fit whitespace-nowrap bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground" disabled={test_cases.length === 0} onClick={function () {
                var runTestRequest = {
                    functions: [
                        {
                            name: func.name.value,
                            run_all_available_tests: filter === '' ? true : false,
                            tests: test_cases.map(function (test_case) { return ({
                                name: test_case.name.value,
                                impls: func.impls.map(function (i) { return i.name.value; }),
                            }); }),
                        },
                    ],
                };
                vscode_1.vscode.postMessage({
                    command: 'runTest',
                    data: {
                        root_path: root_path,
                        tests: runTestRequest,
                    },
                });
            }}>
              <>Run {filter ? test_cases.length : 'all'}</>
            </button_1.Button>
          </>)}
      </div>
      <div className="flex flex-col py-2 divide-y gap-y-1 divide-vscode-textSeparator-foreground">
        {/* <pre>{JSON.stringify(input_json_schema, null, 2)}</pre> */}
        <EditTestCaseForm testCase={undefined} schema={input_json_schema} func={func} getTestParams={getTestParams}>
          <button_1.Button className="flex flex-row text-sm gap-x-2 bg-vscode-dropdown-background text-vscode-dropdown-foreground hover:opacity-90 hover:bg-vscode-dropdown-background">
            <lucide_react_1.PlusIcon size={16}/>
            <div>Add test case</div>
          </button_1.Button>
        </EditTestCaseForm>
        {test_cases.map(function (test_case) { return (<div key={test_case.name.value} className="py-1 group">
            <div className="flex flex-row items-center justify-between">
              <div className="flex flex-row items-center justify-center gap-x-1">
                <button_1.Button variant={'ghost'} size={'icon'} className="p-1 rounded-md w-fit h-fit bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground" disabled={impl === undefined || (test_results === null || test_results === void 0 ? void 0 : test_results.run_status) === 'RUNNING'} onClick={function () {
                var runTestRequest = {
                    functions: [
                        {
                            name: func.name.value,
                            tests: [
                                {
                                    name: test_case.name.value,
                                    impls: impl ? [impl.name.value] : [],
                                },
                            ],
                        },
                    ],
                };
                vscode_1.vscode.postMessage({
                    command: 'runTest',
                    data: {
                        root_path: root_path,
                        tests: runTestRequest,
                    },
                });
            }}>
                  <lucide_react_1.Play size={10}/>
                </button_1.Button>
                {/* IDK why it doesnt truncate. Probably cause of the allotment */}
                <div className="flex w-full flex-nowrap">
                  <span className="h-[24px] max-w-[120px] text-center align-middle overflow-hidden flex-1 truncate">
                    {test_case.name.value}
                  </span>
                  <div className="hidden gap-x-1 group-hover:flex">
                    <EditTestCaseForm testCase={test_case} schema={input_json_schema} func={func} getTestParams={getTestParams}>
                      <button_1.Button variant={'ghost'} size="icon" className="p-1 w-fit h-fit hover:bg-vscode-button-secondaryHoverBackground">
                        <lucide_react_1.Edit2 className="w-3 h-3 text-vscode-descriptionForeground"/>
                      </button_1.Button>
                    </EditTestCaseForm>
                    <tooltip_1.Tooltip delayDuration={100}>
                      <tooltip_1.TooltipTrigger asChild>
                        <button_1.Button variant={'ghost'} size={'icon'} className="p-1 w-fit h-fit text-vscode-descriptionForeground hover:bg-vscode-button-secondaryHoverBackground" onClick={function () {
                vscode_1.vscode.postMessage({ command: 'jumpToFile', data: test_case.name });
            }}>
                          <lucide_react_1.FileJson2 size={14}/>
                        </button_1.Button>
                      </tooltip_1.TooltipTrigger>
                      <tooltip_1.TooltipContent className="flex flex-col gap-y-1">Open test file</tooltip_1.TooltipContent>
                    </tooltip_1.Tooltip>
                    <tooltip_1.Tooltip delayDuration={100}>
                      <tooltip_1.TooltipTrigger>
                        <EditTestCaseForm testCase={test_case} schema={input_json_schema} func={func} getTestParams={getTestParams} duplicate>
                          <button_1.Button variant={'ghost'} size="icon" className="p-1 w-fit h-fit hover:bg-vscode-button-secondaryHoverBackground">
                            <lucide_react_1.Copy size={12}/>
                          </button_1.Button>
                        </EditTestCaseForm>
                      </tooltip_1.TooltipTrigger>
                      <tooltip_1.TooltipContent className="flex flex-col gap-y-1">Duplicate</tooltip_1.TooltipContent>
                    </tooltip_1.Tooltip>
                  </div>
                </div>
              </div>
              <button_1.Button variant={'ghost'} size={'icon'} className="p-1 w-fit h-fit text-vscode-input-foreground" onClick={function () {
                vscode_1.vscode.postMessage({
                    command: 'removeTest',
                    data: {
                        root_path: root_path,
                        funcName: func.name.value,
                        testCaseName: test_case.name,
                    },
                });
            }}>
                <lucide_react_1.X size={10}/>
              </button_1.Button>
            </div>
            <EditTestCaseForm testCase={test_case} schema={input_json_schema} func={func} getTestParams={getTestParams}>
              <button_1.Button variant={'ghost'} className="items-start justify-start w-full px-1 py-1 text-left hover:bg-vscode-button-secondaryHoverBackground h-fit">
                <TestCaseCard content={test_case.content} testCaseName={test_case.name.value}/>
              </button_1.Button>
            </EditTestCaseForm>
          </div>); })}
      </div>
    </>);
};
var EditTestCaseForm = function (_a) {
    var testCase = _a.testCase, schema = _a.schema, func = _a.func, getTestParams = _a.getTestParams, children = _a.children, duplicate = _a.duplicate;
    var root_path = (0, react_2.useContext)(ASTProvider_1.ASTContext).root_path;
    // TODO, actually fix this for named args
    var formData = (0, react_2.useMemo)(function () {
        var _a;
        if (testCase === undefined)
            return {};
        try {
            return JSON.parse(testCase === null || testCase === void 0 ? void 0 : testCase.content);
        }
        catch (e) {
            console.log('Error parsing data\n' + JSON.stringify(testCase), e);
            return (_a = testCase === null || testCase === void 0 ? void 0 : testCase.content) !== null && _a !== void 0 ? _a : 'null';
        }
    }, [testCase === null || testCase === void 0 ? void 0 : testCase.content]);
    var _b = (0, react_2.useState)(false), showForm = _b[0], setShowForm = _b[1];
    var _c = (0, react_2.useState)(duplicate ? undefined : testCase === null || testCase === void 0 ? void 0 : testCase.name.value), testName = _c[0], setTestName = _c[1];
    return (<dialog_1.Dialog open={showForm} onOpenChange={setShowForm}>
      <dialog_1.DialogTrigger asChild={true}>{children}</dialog_1.DialogTrigger>
      <dialog_1.DialogContent className="max-h-screen overflow-y-scroll bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip">
        <dialog_1.DialogHeader className="flex flex-row items-center gap-x-4">
          <dialog_1.DialogTitle className="text-xs font-semibold">{duplicate ? 'Duplicate test' : 'Edit test'}</dialog_1.DialogTitle>

          <div className="flex flex-row items-center pb-1 gap-x-2">
            {testCase === undefined || duplicate ? (<react_1.VSCodeTextField className="w-32" value={testName === undefined ? '' : testName} placeholder="Enter test name" onInput={function (e) {
                setTestName(e.currentTarget.value);
            }}/>) : (
        // for now we dont support renaming existing test
        <div>{testName}</div>)}
          </div>
        </dialog_1.DialogHeader>
        <core_1.default schema={schema} formData={formData} validator={validator_ajv8_1.default} uiSchema={uiSchema} templates={{
            BaseInputTemplate: MyBaseInputTemplate,
            FieldTemplate: MyFieldTemplate,
            ObjectFieldTemplate: MyObjectTemplate,
            ButtonTemplates: {
                AddButton: AddButton,
                // RemoveButton,
                SubmitButton: SubmitButton,
            },
            ArrayFieldTitleTemplate: ArrayFieldTitleTemplate,
            ArrayFieldItemTemplate: ArrayFieldItemTemplate,
        }} onSubmit={function (data) {
            var testCaseToSave = testCase !== null && testCase !== void 0 ? testCase : {
                name: {
                    value: 'new',
                    source_file: '',
                    start: 0,
                    end: 0,
                },
                content: 'null',
            };
            vscode_1.vscode.postMessage({
                command: 'saveTest',
                data: {
                    root_path: root_path,
                    funcName: func.name.value,
                    testCaseName: testName, // a stringspan or string
                    params: getTestParams(__assign(__assign({}, (testCase !== null && testCase !== void 0 ? testCase : {
                        name: {
                            value: 'new',
                            source_file: '',
                            start: 0,
                            end: 0,
                        },
                        content: 'null',
                    })), { content: JSON.stringify(data.formData, null, 2) })),
                },
            });
            setShowForm(false);
        }}/>
      </dialog_1.DialogContent>
    </dialog_1.Dialog>);
};
var TestCaseCard = function (_a) {
    var testCaseName = _a.testCaseName, content = _a.content;
    return (<div className="flex flex-col gap-2 text-xs text-left text-vscode-descriptionForeground">
      <div className="break-all text-balance">
        {content.substring(0, 120)}
        {content.length > 120 && '...'}
      </div>
    </div>);
};
exports.default = TestCasePanel;
