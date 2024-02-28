"use strict";
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
exports.DialogDescription = exports.DialogTitle = exports.DialogFooter = exports.DialogHeader = exports.DialogContent = exports.DialogClose = exports.DialogTrigger = exports.DialogOverlay = exports.DialogPortal = exports.Dialog = void 0;
var React = require("react");
var DialogPrimitive = require("@radix-ui/react-dialog");
var react_icons_1 = require("@radix-ui/react-icons");
var utils_1 = require("@/lib/utils");
var Dialog = DialogPrimitive.Root;
exports.Dialog = Dialog;
var DialogTrigger = DialogPrimitive.Trigger;
exports.DialogTrigger = DialogTrigger;
var DialogPortal = DialogPrimitive.Portal;
exports.DialogPortal = DialogPortal;
var DialogClose = DialogPrimitive.Close;
exports.DialogClose = DialogClose;
var DialogOverlay = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<DialogPrimitive.Overlay ref={ref} className={(0, utils_1.cn)('fixed inset-0 z-50 bg-background/80 backdrop-brightness-50 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0', className)} {...props}/>);
});
exports.DialogOverlay = DialogOverlay;
DialogOverlay.displayName = DialogPrimitive.Overlay.displayName;
var DialogContent = React.forwardRef(function (_a, ref) {
    var className = _a.className, children = _a.children, props = __rest(_a, ["className", "children"]);
    return (<DialogPortal>
    <DialogOverlay />
    <DialogPrimitive.Content ref={ref} className={(0, utils_1.cn)('fixed left-[50%] top-[50%] z-50 grid w-full max-w-lg translate-x-[-50%] translate-y-[-50%] gap-4 border bg-background p-6 shadow-lg duration-200 data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[state=closed]:slide-out-to-left-1/2 data-[state=closed]:slide-out-to-top-[48%] data-[state=open]:slide-in-from-left-1/2 data-[state=open]:slide-in-from-top-[48%] sm:rounded-lg', className)} {...props}>
      {children}
      <DialogPrimitive.Close className="absolute right-4 top-4 rounded-sm opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground">
        <react_icons_1.Cross2Icon className="w-4 h-4"/>
        <span className="sr-only">Close</span>
      </DialogPrimitive.Close>
    </DialogPrimitive.Content>
  </DialogPortal>);
});
exports.DialogContent = DialogContent;
DialogContent.displayName = DialogPrimitive.Content.displayName;
var DialogHeader = function (_a) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<div className={(0, utils_1.cn)('flex flex-col space-y-1.5 text-center sm:text-left', className)} {...props}/>);
};
exports.DialogHeader = DialogHeader;
DialogHeader.displayName = 'DialogHeader';
var DialogFooter = function (_a) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<div className={(0, utils_1.cn)('flex flex-col-reverse sm:flex-row sm:justify-end sm:space-x-2', className)} {...props}/>);
};
exports.DialogFooter = DialogFooter;
DialogFooter.displayName = 'DialogFooter';
var DialogTitle = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<DialogPrimitive.Title ref={ref} className={(0, utils_1.cn)('text-lg font-semibold leading-none tracking-tight', className)} {...props}/>);
});
exports.DialogTitle = DialogTitle;
DialogTitle.displayName = DialogPrimitive.Title.displayName;
var DialogDescription = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<DialogPrimitive.Description ref={ref} className={(0, utils_1.cn)('text-sm text-muted-foreground', className)} {...props}/>);
});
exports.DialogDescription = DialogDescription;
DialogDescription.displayName = DialogPrimitive.Description.displayName;
