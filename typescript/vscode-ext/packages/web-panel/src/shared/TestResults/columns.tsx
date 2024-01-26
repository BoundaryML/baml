'use client'
import { CaretSortIcon } from '@radix-ui/react-icons'
import { Button } from '@/components/ui/button'
import { TestResult, TestStatus } from '@baml/common'
import { ColumnDef } from '@tanstack/react-table'
import { VSCodeLink, VSCodeProgressRing } from '@vscode/webview-ui-toolkit/react'
import { Braces, ExternalLink, File } from 'lucide-react'
import { PropsWithChildren, useState } from 'react'
import JsonView from 'react18-json-view'
import 'react18-json-view/src/style.css'
import { parseGlooObject } from '../schemaUtils'
import { Toggle } from '@/components/ui/toggle'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'

const TestStatusIcon: React.FC<PropsWithChildren<{ testStatus: TestStatus }>> = ({ testStatus, children }) => {
  return (
    <div className="text-vscode-descriptionForeground">
      {
        {
          [TestStatus.Compiling]: 'Compiling',
          [TestStatus.Queued]: 'Queued',
          [TestStatus.Running]: <VSCodeProgressRing className="h-4" />,
          [TestStatus.Passed]: (
            <div className="flex flex-row items-center gap-1">
              <div className="text-vscode-testing-iconPassed">Passed</div>
              {children}
            </div>
          ),
          [TestStatus.Failed]: (
            <div className="flex flex-row items-center gap-1">
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
    // accessorKey: 'testName',
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
    cell: ({ getValue, row, cell }) => {
      return (
        <div className="flex flex-row items-center gap-1 text-center">
          <div className="">{row.original.testName}</div>
          <div className="text-xs text-vscode-descriptionForeground">({row.original.implName})</div>
        </div>
      )
    },
    accessorFn: (row) => `${row.testName}-${row.implName}`,
    id: 'testName-implName',
  },
  // {
  //   header: ({ column }) => {
  //     return (
  //       <Button
  //         variant="ghost"
  //         className="hover:bg-vscode-list-hoverBackground hover:text-vscode-list-hoverForeground"
  //         onClick={() => column.toggleSorting(column.getIsSorted() === 'asc')}
  //       >
  //         impl
  //         <CaretSortIcon className="w-4 h-4 ml-2" />
  //       </Button>
  //     )
  //   },
  //   cell: ({ row }) => (
  //     <div className="flex flex-row gap-1">
  //       <div className="lowercase">{row.getValue('implName')}</div>
  //     </div>
  //   ),
  //   accessorKey: 'implName',
  // },
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
      const [showJson, setShowJson] = useState(true)

      return (
        <div className="flex flex-col w-full p-0 text-xs">
          <TestStatusIcon testStatus={val.status}>
            {val.url && (
              <VSCodeLink href={val.url}>
                <ExternalLink className="w-4 h-4" />
              </VSCodeLink>
            )}
          </TestStatusIcon>
          {val.error && (
            <pre className="break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5">
              {pretty_error(val.error)}
            </pre>
          )}
          {val.render && (
            <pre className="break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5 relative bg-[#1E1E1E]">
              <div className="absolute top-0 right-0 p-1 text-vscode-button-secondaryForeground">
                <TooltipProvider>
                  <Tooltip delayDuration={50}>
                    <TooltipTrigger asChild>
                      <Toggle
                        className="hover:bg-vscode-button-secondaryHoverBackground data-[state=on]:bg-vscode-button-secondaryHoverBackground data-[state=on]:text-vscode-button-secondaryForeground px-1 py-1 opacity-60 h-fit bg-vscode-button-secondaryBackground text-vscode-button-secondaryForeground"
                        pressed={showJson}
                        onPressedChange={(p) => setShowJson(p)}
                      >
                        <Braces className="w-3 h-3" />
                      </Toggle>
                    </TooltipTrigger>
                    <TooltipContent>{showJson ? 'Show Raw LLM Output' : 'Show Parsed Value'}</TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              </div>
              {!showJson ? (
                val.raw
              ) : (
                <JsonView
                  enableClipboard={false}
                  className="bg-[#1E1E1E]"
                  theme="a11y"
                  collapseStringsAfterLength={600}
                  src={parseGlooObject({
                    value: pretty_stringify(val.render),
                  })}
                />
              )}
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
