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
exports.ScrollBar = exports.ScrollArea = void 0;
var React = require("react");
var ScrollAreaPrimitive = require("@radix-ui/react-scroll-area");
var utils_1 = require("@/lib/utils");
var ScrollArea = React.forwardRef(function (_a, ref) {
    var className = _a.className, children = _a.children, props = __rest(_a, ["className", "children"]);
    return (<ScrollAreaPrimitive.Root ref={ref} className={(0, utils_1.cn)("relative overflow-hidden", className)} {...props}>
    <ScrollAreaPrimitive.Viewport className="h-full w-full rounded-[inherit]">
      {children}
    </ScrollAreaPrimitive.Viewport>
    <ScrollBar />
    <ScrollAreaPrimitive.Corner />
  </ScrollAreaPrimitive.Root>);
});
exports.ScrollArea = ScrollArea;
ScrollArea.displayName = ScrollAreaPrimitive.Root.displayName;
var ScrollBar = React.forwardRef(function (_a, ref) {
    var className = _a.className, _b = _a.orientation, orientation = _b === void 0 ? "vertical" : _b, props = __rest(_a, ["className", "orientation"]);
    return (<ScrollAreaPrimitive.ScrollAreaScrollbar ref={ref} orientation={orientation} className={(0, utils_1.cn)("flex touch-none select-none transition-colors", orientation === "vertical" &&
            "h-full w-2.5 border-l border-l-transparent p-[1px]", orientation === "horizontal" &&
            "h-2.5 flex-col border-t border-t-transparent p-[1px]", className)} {...props}>
    <ScrollAreaPrimitive.ScrollAreaThumb className="relative flex-1 rounded-full bg-border"/>
  </ScrollAreaPrimitive.ScrollAreaScrollbar>);
});
exports.ScrollBar = ScrollBar;
ScrollBar.displayName = ScrollAreaPrimitive.ScrollAreaScrollbar.displayName;
