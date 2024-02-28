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
exports.HoverCardContent = exports.HoverCardTrigger = exports.HoverCard = void 0;
var React = require("react");
var HoverCardPrimitive = require("@radix-ui/react-hover-card");
var utils_1 = require("@/lib/utils");
var HoverCard = HoverCardPrimitive.Root;
exports.HoverCard = HoverCard;
var HoverCardTrigger = HoverCardPrimitive.Trigger;
exports.HoverCardTrigger = HoverCardTrigger;
var HoverCardContent = React.forwardRef(function (_a, ref) {
    var className = _a.className, _b = _a.align, align = _b === void 0 ? "center" : _b, _c = _a.sideOffset, sideOffset = _c === void 0 ? 4 : _c, props = __rest(_a, ["className", "align", "sideOffset"]);
    return (<HoverCardPrimitive.Content ref={ref} align={align} sideOffset={sideOffset} className={(0, utils_1.cn)("z-50 w-64 rounded-md border bg-popover p-4 text-popover-foreground shadow-md outline-none data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2", className)} {...props}/>);
});
exports.HoverCardContent = HoverCardContent;
HoverCardContent.displayName = HoverCardPrimitive.Content.displayName;
