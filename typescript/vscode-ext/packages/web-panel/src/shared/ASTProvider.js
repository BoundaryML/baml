"use strict";
var __spreadArray = (this && this.__spreadArray) || function (to, from, pack) {
    if (pack || arguments.length === 2) for (var i = 0, l = from.length, ar; i < l; i++) {
        if (ar || !(i in from)) {
            if (!ar) ar = Array.prototype.slice.call(from, 0, i);
            ar[i] = from[i];
        }
    }
    return to.concat(ar || Array.prototype.slice.call(from));
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.ASTProvider = exports.ASTContext = void 0;
var ErrorFallback_1 = require("@/utils/ErrorFallback");
var react_1 = require("@vscode/webview-ui-toolkit/react");
var react_2 = require("react");
exports.ASTContext = (0, react_2.createContext)({
    projects: [],
    selectedProjectId: '',
    root_path: '',
    db: {
        functions: [],
        classes: [],
        clients: [],
        enums: [],
    },
    jsonSchema: {
        definitions: {},
    },
    test_log: undefined,
    test_results: undefined,
    selections: {
        selectedFunction: undefined,
        selectedImpl: undefined,
        selectedTestCase: undefined,
        showTests: true,
    },
    setSelection: function () { },
});
function useSelectionSetup() {
    var _a = (0, react_2.useState)(undefined), selectedProjectId = _a[0], setSelectedProjectId = _a[1];
    var _b = (0, react_2.useState)(undefined), selectedFunction = _b[0], setSelectedFunction = _b[1];
    var _c = (0, react_2.useState)(undefined), selectedImpl = _c[0], setSelectedImpl = _c[1];
    var _d = (0, react_2.useState)(undefined), selectedTestCase = _d[0], setSelectedTestCase = _d[1];
    var _e = (0, react_2.useState)(true), showTests = _e[0], setShowTests = _e[1];
    var setSelectionFunction = (0, react_2.useCallback)(function (selectedProjectId, functionName, implName, testCaseName, showTests) {
        if (selectedProjectId) {
            setSelectedProjectId(selectedProjectId);
        }
        if (functionName) {
            setSelectedFunction(functionName);
            setSelectedImpl(implName);
            setSelectedTestCase(testCaseName);
        }
        else {
            if (implName) {
                setSelectedImpl(implName);
            }
            if (testCaseName) {
                setSelectedTestCase(testCaseName);
            }
        }
        if (showTests !== undefined) {
            setShowTests(showTests);
        }
        else if (testCaseName !== undefined) {
            setShowTests(true);
        }
    }, []);
    return {
        selectedProjectId: selectedProjectId,
        selectedFunction: selectedFunction,
        selectedImpl: selectedImpl,
        selectedTestCase: selectedTestCase,
        showTests: showTests,
        setSelection: setSelectionFunction,
    };
}
var ASTProvider = function (_a) {
    var children = _a.children;
    var _b = (0, react_2.useState)([]), projects = _b[0], setProjects = _b[1];
    var _c = (0, react_2.useState)(undefined), testResults = _c[0], setTestResults = _c[1];
    var _d = useSelectionSetup(), selectedProjectId = _d.selectedProjectId, selectedFunction = _d.selectedFunction, selectedImpl = _d.selectedImpl, selectedTestCase = _d.selectedTestCase, showTests = _d.showTests, setSelection = _d.setSelection;
    var _e = (0, react_2.useState)(undefined), testLog = _e[0], setTestLog = _e[1];
    var selectedState = (0, react_2.useMemo)(function () {
        if (selectedProjectId === undefined)
            return undefined;
        var match = projects.find(function (project) { return project.root_dir === selectedProjectId; });
        if (match) {
            var jsonSchema = {
                definitions: Object.fromEntries(__spreadArray(__spreadArray([], match.db.classes.flatMap(function (c) { return Object.entries(c.jsonSchema); }), true), match.db.enums.flatMap(function (c) { return Object.entries(c.jsonSchema); }), true)),
            };
            return {
                projects: projects,
                selectedProjectId: selectedProjectId,
                root_path: match.root_dir,
                db: match.db,
                jsonSchema: jsonSchema,
                test_results: testResults,
                test_log: testLog,
                selections: {
                    selectedFunction: selectedFunction,
                    selectedImpl: selectedImpl,
                    selectedTestCase: selectedTestCase,
                    showTests: showTests,
                },
                setSelection: setSelection,
            };
        }
        return undefined;
    }, [
        projects,
        selectedProjectId,
        testResults,
        selectedFunction,
        selectedImpl,
        selectedTestCase,
        showTests,
        setSelection,
    ]);
    (0, react_2.useEffect)(function () {
        if (projects.length === 0)
            return;
        if (selectedProjectId === undefined) {
            setSelection(projects[0].root_dir, undefined, undefined, undefined, undefined);
        }
    }, [selectedProjectId, projects]);
    (0, react_2.useEffect)(function () {
        var fn = function (event) {
            var command = event.data.command;
            var messageContent = event.data.content;
            switch (command) {
                case 'test-stdout': {
                    if (messageContent === '<BAML_RESTART>') {
                        setTestLog(undefined);
                    }
                    else {
                        setTestLog(function (prev) { return (prev ? prev + messageContent : messageContent); });
                    }
                }
                case 'setDb': {
                    console.log('parser db updated', messageContent);
                    setProjects(messageContent.map(function (p) { return ({ root_dir: p[0], db: p[1] }); }));
                    break;
                }
                case 'rmDb': {
                    setProjects(function (prev) { return prev.filter(function (project) { return project.root_dir !== messageContent; }); });
                    break;
                }
                case 'setSelectedResource': {
                    var content = messageContent;
                    setSelection(content.projectId, content.functionName, content.implName, content.testCaseName, content.showTests);
                    break;
                }
                case 'test-results': {
                    setTestResults(messageContent);
                    break;
                }
            }
        };
        window.addEventListener('message', fn);
        return function () {
            window.removeEventListener('message', fn);
        };
    }, []);
    return (<main className="w-full h-screen px-0 py-2 overflow-y-clip">
      {selectedState === undefined ? (projects.length === 0 ? (<div>
            No baml projects loaded yet.
            <br />
            Open a baml file or wait for the extension to finish loading!
          </div>) : (<div>
            <h1>Projects</h1>
            <div>
              {projects.map(function (project) { return (<div key={project.root_dir}>
                  <react_1.VSCodeButton onClick={function () { return setSelection(project.root_dir, undefined, undefined, undefined, undefined); }}>
                    {project.root_dir}
                  </react_1.VSCodeButton>
                </div>); })}
            </div>
          </div>)) : (<ErrorFallback_1.default>
          <exports.ASTContext.Provider value={selectedState}>{children}</exports.ASTContext.Provider>
        </ErrorFallback_1.default>)}
    </main>);
};
exports.ASTProvider = ASTProvider;
