'use client'
import { CaretSortIcon } from '@radix-ui/react-icons'
import { Button } from '@/components/ui/button'
import { TestResult, TestStatus } from '@baml/common'
import { ColumnDef } from '@tanstack/react-table'
import { VSCodeLink, VSCodeProgressRing } from '@vscode/webview-ui-toolkit/react'
import { ExternalLink } from 'lucide-react'
import { PropsWithChildren } from 'react'

const TestStatusIcon: React.FC<PropsWithChildren<{ testStatus: TestStatus }>> = ({ testStatus, children }) => {
  return (
    <div className="text-vscode-descriptionForeground">
      {
        {
          [TestStatus.Compiling]: 'Compiling',
          [TestStatus.Queued]: 'Queued',
          [TestStatus.Running]: <VSCodeProgressRing className="h-4" />,
          [TestStatus.Passed]: (
            <div className="flex flex-row gap-1 items-center">
              <div className="text-vscode-testing-iconPassed">Passed</div>
              {children}
            </div>
          ),
          [TestStatus.Failed]: (
            <div className="flex flex-row gap-1 items-center">
              <div className="text-vscode-testing-iconFailed">Failed</div>
              {children}
            </div>
          ),
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
    cell: ({ row }) => (
      <div className="flex flex-row gap-1">
        <div className="lowercase">{row.getValue('implName')}</div>
      </div>
    ),
    accessorKey: 'implName',
  },
  {
    id: 'status',
    accessorFn: (row) => ({
      status: row.status,
      error: row.output.error,
      render: row.output.parsed,
      raw: row.output.raw,
      url: row.url,
    }),
    cell: ({ getValue }) => {
      const val = getValue<{ status: TestStatus; render?: string; error?: string; raw?: string; url?: string }>()

      return (
        <div className="flex flex-col p-0 text-xs">
          <TestStatusIcon testStatus={val.status}>
            {val.url && (
              <VSCodeLink href={val.url}>
                <ExternalLink className="w-4 h-4" />
              </VSCodeLink>
            )}
          </TestStatusIcon>
          {val.error && (
            <pre className="break-words whitespace-pre-wrap max-w-[500px] border-vscode-textSeparator-foreground rounded-md border p-0.5">
              {pretty_error(val.error)}
            </pre>
          )}
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

const pretty_error = (obj: string) => {
  try {
    let err: { error: string } = JSON.parse(obj)
    return err.error
  } catch (e) {
    return obj
  }
}

const pretty_stringify = (obj: string) => {
  try {
    return JSON.stringify(JSON.parse(obj), null, 2)
  } catch (e) {
    return obj
  }
}
