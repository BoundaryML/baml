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
exports.TableCaption = exports.TableCell = exports.TableRow = exports.TableHead = exports.TableFooter = exports.TableBody = exports.TableHeader = exports.Table = void 0;
var React = require("react");
var utils_1 = require("@/lib/utils");
var Table = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<div className="relative w-full overflow-auto">
      <table ref={ref} className={(0, utils_1.cn)('w-full caption-bottom text-sm', className)} {...props}/>
    </div>);
});
exports.Table = Table;
Table.displayName = 'Table';
var TableHeader = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return <thead ref={ref} className={(0, utils_1.cn)('[&_tr]:border-b', className)} {...props}/>;
});
exports.TableHeader = TableHeader;
TableHeader.displayName = 'TableHeader';
var TableBody = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<tbody ref={ref} className={(0, utils_1.cn)('[&_tr:last-child]:border-0', className)} {...props}/>);
});
exports.TableBody = TableBody;
TableBody.displayName = 'TableBody';
var TableFooter = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<tfoot ref={ref} className={(0, utils_1.cn)('border-t bg-muted/50 font-medium [&>tr]:last:border-b-0', className)} {...props}/>);
});
exports.TableFooter = TableFooter;
TableFooter.displayName = 'TableFooter';
var TableRow = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<tr ref={ref} className={(0, utils_1.cn)('border-b transition-colors hover:bg-muted/50 data-[state=selected]:bg-muted', className)} {...props}/>);
});
exports.TableRow = TableRow;
TableRow.displayName = 'TableRow';
var TableHead = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<th ref={ref} className={(0, utils_1.cn)('h-fit px-3 text-left align-middle font-medium text-muted-foreground [&:has([role=checkbox])]:pr-0', className)} {...props}/>);
});
exports.TableHead = TableHead;
TableHead.displayName = 'TableHead';
var TableCell = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<td ref={ref} className={(0, utils_1.cn)('p-4 align-middle [&:has([role=checkbox])]:pr-0', className)} {...props}/>);
});
exports.TableCell = TableCell;
TableCell.displayName = 'TableCell';
var TableCaption = React.forwardRef(function (_a, ref) {
    var className = _a.className, props = __rest(_a, ["className"]);
    return (<caption ref={ref} className={(0, utils_1.cn)('mt-4 text-sm text-muted-foreground', className)} {...props}/>);
});
exports.TableCaption = TableCaption;
TableCaption.displayName = 'TableCaption';
