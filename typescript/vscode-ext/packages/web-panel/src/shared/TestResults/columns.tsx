'use client'
import { CaretSortIcon } from '@radix-ui/react-icons'
import { Button } from '@/components/ui/button'
import { TestResult, TestStatus } from '@baml/common'
import { ColumnDef } from '@tanstack/react-table'

export const columns: ColumnDef<TestResult>[] = [
  {
    accessorKey: 'testName',
    header: 'Test Case',
  },
  {
    header: ({ column }) => {
      return (
        <Button variant="ghost" onClick={() => column.toggleSorting(column.getIsSorted() === 'asc')}>
          impl
          <CaretSortIcon className="ml-2 h-4 w-4" />
        </Button>
      )
    },
    cell: ({ row }) => <div className="lowercase">{row.getValue('implName')}</div>,
    accessorKey: 'implName',
  },
  {
    id: 'status',
    accessorFn: (row) => row.status + ' ' + JSON.stringify(row.output),
    cell: ({ getValue }) => <div className="lowercase">{getValue()}</div>,
    header: 'Status',
  },
]
