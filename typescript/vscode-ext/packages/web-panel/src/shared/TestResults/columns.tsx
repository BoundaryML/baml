'use client'
import { CaretSortIcon } from '@radix-ui/react-icons'
import { Button } from '@/components/ui/button'
import { TestResult, TestStatus } from '@baml/common'
import { ColumnDef } from '@tanstack/react-table'
import { VSCodeProgressRing } from '@vscode/webview-ui-toolkit/react'

const TestStatusIcon = ({ testStatus }: { testStatus: TestStatus }) => {
  return (
    <div className="text-vscode-descriptionForeground">
      {
        {
          [TestStatus.Queued]: 'Queued',
          [TestStatus.Running]: <VSCodeProgressRing className="h-4" />,
          [TestStatus.Passed]: <div className="text-vscode-testing-iconPassed">Passed</div>,
          [TestStatus.Failed]: <div className="text-vscode-testing-iconFailed">Failed</div>,
        }[testStatus]
      }
    </div>
  )
}

export const columns: ColumnDef<TestResult>[] = [
  {
    accessorKey: 'testName',
    header: ({ column }) => {
      return (
        <Button
          variant="ghost"
          className="hover:bg-vscode-list-hoverBackground hover:text-vscode-list-hoverForeground"
          onClick={() => column.toggleSorting(column.getIsSorted() === 'asc')}
        >
          Test Case
          <CaretSortIcon className="w-4 h-4 ml-2" />
        </Button>
      )
    },
  },
  {
    header: ({ column }) => {
      return (
        <Button
          variant="ghost"
          className="hover:bg-vscode-list-hoverBackground hover:text-vscode-list-hoverForeground"
          onClick={() => column.toggleSorting(column.getIsSorted() === 'asc')}
        >
          impl
          <CaretSortIcon className="w-4 h-4 ml-2" />
        </Button>
      )
    },
    cell: ({ row }) => <div className="lowercase">{row.getValue('implName')}</div>,
    accessorKey: 'implName',
  },
  {
    id: 'status',
    accessorFn: (row) => ({ status: row.status, render: row.output.parsed ?? row.output.error, raw: row.output.raw }),
    cell: ({ getValue }) => {
      const val = getValue<{ status: TestStatus; render?: string; raw?: string }>()

      return (
        <div className="flex flex-col p-0 text-xs">
          <TestStatusIcon testStatus={val.status} />
          {val.render && (
            <pre className="break-words whitespace-pre-wrap max-w-[500px] border-vscode-textSeparator-foreground rounded-md border p-0.5">
              {pretty_stringify(val.render)}
            </pre>
          )}
        </div>
      )
    },
    header: 'Status',
  },
]

const pretty_stringify = (obj: string) => {
  try {
    return JSON.stringify(JSON.parse(obj), null, 2)
  } catch (e) {
    return obj
  }
}
