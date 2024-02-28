"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ProjectToggle = void 0;
var react_1 = require("react");
var ASTProvider_1 = require("./ASTProvider");
var react_2 = require("@vscode/webview-ui-toolkit/react");
var dialog_1 = require("@/components/ui/dialog");
var button_1 = require("@/components/ui/button");
var ProjectPanel = function (_a) {
    var onClick = _a.onClick;
    var _b = (0, react_1.useContext)(ASTProvider_1.ASTContext), projects = _b.projects, selectedProjectId = _b.selectedProjectId, setSelection = _b.setSelection;
    return (<div>
      <h1>Projects</h1>
      <div>
        {projects.map(function (project) { return (<div key={project.root_dir}>
            <react_2.VSCodeButton onClick={function () {
                setSelection(project.root_dir, undefined, undefined, undefined, undefined);
                onClick === null || onClick === void 0 ? void 0 : onClick();
            }}>
              {project.root_dir}
            </react_2.VSCodeButton>
          </div>); })}
      </div>
    </div>);
};
var ProjectToggle = function () {
    var _a;
    var _b = (0, react_1.useState)(false), show = _b[0], setShow = _b[1];
    var _c = (0, react_1.useContext)(ASTProvider_1.ASTContext), projects = _c.projects, selectedProjectId = _c.selectedProjectId, setSelection = _c.setSelection;
    if (projects.length <= 1) {
        return null;
    }
    return (<dialog_1.Dialog open={show} onOpenChange={setShow}>
      <dialog_1.DialogTrigger asChild={true}>
        <button_1.Button variant="outline" className="p-1 text-xs w-fit h-fit border-vscode-textSeparator-foreground truncate">
          {(_a = selectedProjectId === null || selectedProjectId === void 0 ? void 0 : selectedProjectId.split('/').at(-2)) !== null && _a !== void 0 ? _a : 'Project'}
        </button_1.Button>
      </dialog_1.DialogTrigger>
      <dialog_1.DialogContent className="max-h-screen overflow-y-scroll bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip">
        <ProjectPanel onClick={function () { return setShow(false); }}/>
      </dialog_1.DialogContent>
    </dialog_1.Dialog>);
};
exports.ProjectToggle = ProjectToggle;
exports.default = ProjectPanel;
