'use client';
"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DataTable = void 0;
var react_table_1 = require("@tanstack/react-table");
var table_1 = require("@/components/ui/table");
var react_1 = require("react");
function DataTable(_a) {
    var _b;
    var columns = _a.columns, data = _a.data;
    var _c = (0, react_1.useState)([]), sorting = _c[0], setSorting = _c[1];
    var table = (0, react_table_1.useReactTable)({
        data: data,
        columns: columns,
        getCoreRowModel: (0, react_table_1.getCoreRowModel)(),
        onSortingChange: setSorting,
        getSortedRowModel: (0, react_table_1.getSortedRowModel)(),
        state: {
            sorting: sorting,
        },
    });
    return (<div className="w-full text-xs rounded-md border-vscode-input-border">
      <table_1.Table className="w-full">
        <table_1.TableHeader className="p-0 border-0 gap-x-1">
          {table.getHeaderGroups().map(function (headerGroup) { return (<table_1.TableRow key={headerGroup.id} className="py-1 hover:bg-vscode-list-hoverBackground border-vscode-textSeparator-foreground">
              {headerGroup.headers.map(function (header) {
                if (header.index === 1) {
                    return (<table_1.TableHead key={header.id} className="w-full pl-2 text-left">
                      {header.isPlaceholder ? null : (0, react_table_1.flexRender)(header.column.columnDef.header, header.getContext())}
                    </table_1.TableHead>);
                }
                return (<table_1.TableHead key={header.id} className="pl-2 text-left w-fit">
                    {header.isPlaceholder ? null : (0, react_table_1.flexRender)(header.column.columnDef.header, header.getContext())}
                  </table_1.TableHead>);
            })}
            </table_1.TableRow>); })}
        </table_1.TableHeader>
        <table_1.TableBody className="border-0 divide-0">
          {((_b = table.getRowModel().rows) === null || _b === void 0 ? void 0 : _b.length) ? (table.getRowModel().rows.map(function (row) { return (<table_1.TableRow key={row.id} data-state={row.getIsSelected() && 'selected'} className="py-1 hover:bg-vscode-list-hoverBackground border-vscode-textSeparator-foreground">
                {row.getVisibleCells().map(function (cell) { return (<table_1.TableCell className="py-1 pl-1" key={cell.id}>
                    {(0, react_table_1.flexRender)(cell.column.columnDef.cell, cell.getContext())}
                  </table_1.TableCell>); })}
              </table_1.TableRow>); })) : (<table_1.TableRow className="py-1 hover:bg-vscode-list-hoverBackground border-vscode-textSeparator-foreground">
              <table_1.TableCell colSpan={columns.length} className="h-24 text-center">
                No results.
              </table_1.TableCell>
            </table_1.TableRow>)}
        </table_1.TableBody>
      </table_1.Table>
    </div>);
}
exports.DataTable = DataTable;
