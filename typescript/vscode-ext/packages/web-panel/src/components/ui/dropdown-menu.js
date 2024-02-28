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
exports.DropdownMenuRadioGroup = exports.DropdownMenuSubTrigger = exports.DropdownMenuSubContent = exports.DropdownMenuSub = exports.DropdownMenuPortal = exports.DropdownMenuGroup = exports.DropdownMenuShortcut = exports.DropdownMenuSeparator = exports.DropdownMenuLabel = exports.DropdownMenuRadioItem = exports.DropdownMenuCheckboxItem = exports.DropdownMenuItem = exports.DropdownMenuContent = exports.DropdownMenuTrigger = exports.DropdownMenu = void 0;
var React = require("react");
var DropdownMenuPrimitive = require("@radix-ui/react-dropdown-menu");
var react_icons_1 = require("@radix-ui/react-icons");
var utils_1 = require("@/lib/utils");
var DropdownMenu = DropdownMenuPrimitive.Root;
exports.DropdownMenu = DropdownMenu;
var DropdownMenuTrigger = DropdownMenuPrimitive.Trigger;
exports.DropdownMenuTrigger = DropdownMenuTrigger;
var DropdownMenuGroup = DropdownMenuPrimitive.Group;
exports.DropdownMenuGroup = DropdownMenuGroup;
var DropdownMenuPortal = DropdownMenuPrimitive.Portal;
exports.DropdownMenuPortal = DropdownMenuPortal;
var DropdownMenuSub = DropdownMenuPrimitive.Sub;
exports.DropdownMenuSub = DropdownMenuSub;
var DropdownMenuRadioGroup = DropdownMenuPrimitive.RadioGroup;
exports.DropdownMenuRadioGroup = DropdownMenuRadioGroup;
var DropdownMenuSubTrigger = React.forwardRef(function (_a, ref) {
    var className = _a.className, inset = _a.inset, children = _a.children, props = __rest(_a, ["className", "inset", "children"]);
    return (<DropdownMenuPrimitive.SubTrigger ref={ref} className={(0, utils_1.cn)("flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none focus:bg-accent data-[state=open]:bg-accent", inset && "pl-8", className)} {...props}>
    {children}
    <react_icons_1.ChevronRightIcon className="ml-auto h-4 w-4"/>
  </DropdownMenuPrimitive.SubTrigger>);
});
exports.DropdownMenuSubTrigger = DropdownMenuSubTrigger;
DropdownMenuSubTrigger.displayName =
    DropdownMenuPrimitive.SubTrigger.displayName;
var DropdownMenuSubContent = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<DropdownMenuPrimitive.SubContent ref={ref} className={(0, utils_1.cn)("z-50 min-w-[8rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-lg data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2", className)} {...props}/>);
});
exports.DropdownMenuSubContent = DropdownMenuSubContent;
DropdownMenuSubContent.displayName =
    DropdownMenuPrimitive.SubContent.displayName;
var DropdownMenuContent = React.forwardRef(function (_a, ref) {
    var className = _a.className, _b = _a.sideOffset, sideOffset = _b === void 0 ? 4 : _b, props = __rest(_a, ["className", "sideOffset"]);
    return (<DropdownMenuPrimitive.Portal>
    <DropdownMenuPrimitive.Content ref={ref} sideOffset={sideOffset} className={(0, utils_1.cn)("z-50 min-w-[8rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md", "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2", className)} {...props}/>
  </DropdownMenuPrimitive.Portal>);
});
exports.DropdownMenuContent = DropdownMenuContent;
DropdownMenuContent.displayName = DropdownMenuPrimitive.Content.displayName;
var DropdownMenuItem = React.forwardRef(function (_a, ref) {
    var className = _a.className, inset = _a.inset, props = __rest(_a, ["className", "inset"]);
    return (<DropdownMenuPrimitive.Item ref={ref} className={(0, utils_1.cn)("relative flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50", inset && "pl-8", className)} {...props}/>);
});
exports.DropdownMenuItem = DropdownMenuItem;
DropdownMenuItem.displayName = DropdownMenuPrimitive.Item.displayName;
var DropdownMenuCheckboxItem = React.forwardRef(function (_a, ref) {
    var className = _a.className, children = _a.children, checked = _a.checked, props = __rest(_a, ["className", "children", "checked"]);
    return (<DropdownMenuPrimitive.CheckboxItem ref={ref} className={(0, utils_1.cn)("relative flex cursor-default select-none items-center rounded-sm py-1.5 pl-8 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50", className)} checked={checked} {...props}>
    <span className="absolute left-2 flex h-3.5 w-3.5 items-center justify-center">
      <DropdownMenuPrimitive.ItemIndicator>
        <react_icons_1.CheckIcon className="h-4 w-4"/>
      </DropdownMenuPrimitive.ItemIndicator>
    </span>
    {children}
  </DropdownMenuPrimitive.CheckboxItem>);
});
exports.DropdownMenuCheckboxItem = DropdownMenuCheckboxItem;
DropdownMenuCheckboxItem.displayName =
    DropdownMenuPrimitive.CheckboxItem.displayName;
var DropdownMenuRadioItem = React.forwardRef(function (_a, ref) {
    var className = _a.className, children = _a.children, props = __rest(_a, ["className", "children"]);
    return (<DropdownMenuPrimitive.RadioItem ref={ref} className={(0, utils_1.cn)("relative flex cursor-default select-none items-center rounded-sm py-1.5 pl-8 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50", className)} {...props}>
    <span className="absolute left-2 flex h-3.5 w-3.5 items-center justify-center">
      <DropdownMenuPrimitive.ItemIndicator>
        <react_icons_1.DotFilledIcon className="h-4 w-4 fill-current"/>
      </DropdownMenuPrimitive.ItemIndicator>
    </span>
    {children}
  </DropdownMenuPrimitive.RadioItem>);
});
exports.DropdownMenuRadioItem = DropdownMenuRadioItem;
DropdownMenuRadioItem.displayName = DropdownMenuPrimitive.RadioItem.displayName;
var DropdownMenuLabel = React.forwardRef(function (_a, ref) {
    var className = _a.className, inset = _a.inset, props = __rest(_a, ["className", "inset"]);
    return (<DropdownMenuPrimitive.Label ref={ref} className={(0, utils_1.cn)("px-2 py-1.5 text-sm font-semibold", inset && "pl-8", className)} {...props}/>);
});
exports.DropdownMenuLabel = DropdownMenuLabel;
DropdownMenuLabel.displayName = DropdownMenuPrimitive.Label.displayName;
var DropdownMenuSeparator = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<DropdownMenuPrimitive.Separator ref={ref} className={(0, utils_1.cn)("-mx-1 my-1 h-px bg-muted", className)} {...props}/>);
});
exports.DropdownMenuSeparator = DropdownMenuSeparator;
DropdownMenuSeparator.displayName = DropdownMenuPrimitive.Separator.displayName;
var DropdownMenuShortcut = function (_a) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<span className={(0, utils_1.cn)("ml-auto text-xs tracking-widest opacity-60", className)} {...props}/>);
};
exports.DropdownMenuShortcut = DropdownMenuShortcut;
DropdownMenuShortcut.displayName = "DropdownMenuShortcut";
