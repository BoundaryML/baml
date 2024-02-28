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
exports.ToastAction = exports.ToastClose = exports.ToastDescription = exports.ToastTitle = exports.Toast = exports.ToastViewport = exports.ToastProvider = void 0;
var React = require("react");
var react_icons_1 = require("@radix-ui/react-icons");
var ToastPrimitives = require("@radix-ui/react-toast");
var class_variance_authority_1 = require("class-variance-authority");
var utils_1 = require("@/lib/utils");
var ToastProvider = ToastPrimitives.Provider;
exports.ToastProvider = ToastProvider;
var ToastViewport = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<ToastPrimitives.Viewport ref={ref} className={(0, utils_1.cn)("fixed top-0 z-[100] flex max-h-screen w-full flex-col-reverse p-4 sm:bottom-0 sm:right-0 sm:top-auto sm:flex-col md:max-w-[420px]", className)} {...props}/>);
});
exports.ToastViewport = ToastViewport;
ToastViewport.displayName = ToastPrimitives.Viewport.displayName;
var toastVariants = (0, class_variance_authority_1.cva)("group pointer-events-auto relative flex w-full items-center justify-between space-x-2 overflow-hidden rounded-md border p-4 pr-6 shadow-lg transition-all data-[swipe=cancel]:translate-x-0 data-[swipe=end]:translate-x-[var(--radix-toast-swipe-end-x)] data-[swipe=move]:translate-x-[var(--radix-toast-swipe-move-x)] data-[swipe=move]:transition-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[swipe=end]:animate-out data-[state=closed]:fade-out-80 data-[state=closed]:slide-out-to-right-full data-[state=open]:slide-in-from-top-full data-[state=open]:sm:slide-in-from-bottom-full", {
    variants: {
        variant: {
            default: "border bg-background text-foreground",
            destructive: "destructive group border-destructive bg-destructive text-destructive-foreground",
        },
    },
    defaultVariants: {
        variant: "default",
    },
});
var Toast = React.forwardRef(function (_a, ref) {
    var className = _a.className, variant = _a.variant, props = __rest(_a, ["className", "variant"]);
    return (<ToastPrimitives.Root ref={ref} className={(0, utils_1.cn)(toastVariants({ variant: variant }), className)} {...props}/>);
});
exports.Toast = Toast;
Toast.displayName = ToastPrimitives.Root.displayName;
var ToastAction = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<ToastPrimitives.Action ref={ref} className={(0, utils_1.cn)("inline-flex h-8 shrink-0 items-center justify-center rounded-md border bg-transparent px-3 text-sm font-medium transition-colors hover:bg-secondary focus:outline-none focus:ring-1 focus:ring-ring disabled:pointer-events-none disabled:opacity-50 group-[.destructive]:border-muted/40 group-[.destructive]:hover:border-destructive/30 group-[.destructive]:hover:bg-destructive group-[.destructive]:hover:text-destructive-foreground group-[.destructive]:focus:ring-destructive", className)} {...props}/>);
});
exports.ToastAction = ToastAction;
ToastAction.displayName = ToastPrimitives.Action.displayName;
var ToastClose = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<ToastPrimitives.Close ref={ref} className={(0, utils_1.cn)("absolute right-1 top-1 rounded-md p-1 text-foreground/50 opacity-0 transition-opacity hover:text-foreground focus:opacity-100 focus:outline-none focus:ring-1 group-hover:opacity-100 group-[.destructive]:text-red-300 group-[.destructive]:hover:text-red-50 group-[.destructive]:focus:ring-red-400 group-[.destructive]:focus:ring-offset-red-600", className)} toast-close="" {...props}>
    <react_icons_1.Cross2Icon className="h-4 w-4"/>
  </ToastPrimitives.Close>);
});
exports.ToastClose = ToastClose;
ToastClose.displayName = ToastPrimitives.Close.displayName;
var ToastTitle = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<ToastPrimitives.Title ref={ref} className={(0, utils_1.cn)("text-sm font-semibold [&+div]:text-xs", className)} {...props}/>);
});
exports.ToastTitle = ToastTitle;
ToastTitle.displayName = ToastPrimitives.Title.displayName;
var ToastDescription = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<ToastPrimitives.Description ref={ref} className={(0, utils_1.cn)("text-sm opacity-90", className)} {...props}/>);
});
exports.ToastDescription = ToastDescription;
ToastDescription.displayName = ToastPrimitives.Description.displayName;
