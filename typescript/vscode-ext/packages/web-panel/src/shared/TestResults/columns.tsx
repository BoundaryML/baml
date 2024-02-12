'use client'
import { CaretSortIcon } from '@radix-ui/react-icons'
import { Button } from '@/components/ui/button'
import { StringSpan, TestResult, TestStatus } from '@baml/common'
import { ColumnDef } from '@tanstack/react-table'
import { VSCodeLink, VSCodeProgressRing } from '@vscode/webview-ui-toolkit/react'
import { Braces, ExternalLink, File } from 'lucide-react'
import { PropsWithChildren, useMemo, useState } from 'react'
import JsonView from 'react18-json-view'
import 'react18-json-view/src/style.css'
import { parseGlooObject } from '../schemaUtils'
import { Toggle } from '@/components/ui/toggle'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { HoverCard, HoverCardContent, HoverCardTrigger } from '@/components/ui/hover-card'
import Link from '../Link'
import { Switch } from '@/components/ui/switch'
import { Label } from '@/components/ui/label'

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

export const columns: ColumnDef<TestResult & { span?: StringSpan }>[] = [
  {
    header: ({ column }) => {
      return (
        <Button
          variant="ghost"
          className="py-1 hover:bg-vscode-list-hoverBackground hover:text-vscode-list-hoverForeground h-fit"
          onClick={() => column.toggleSorting(column.getIsSorted() === 'asc')}
        >
          Test Case
          <CaretSortIcon className="w-4 h-4 ml-2" />
        </Button>
      )
    },
    cell: ({ getValue, row, cell }) => {
      return (
        <HoverCard openDelay={50} closeDelay={0}>
          <HoverCardTrigger>
            <div className="flex flex-row items-center gap-1 text-center w-fit">
              <div className="underline">
                {row.original.span ? (
                  <Link item={row.original.span} display={row.original.testName}>
                    {row.original.testName}
                  </Link>
                ) : (
                  <>{row.original.testName}</>
                )}
              </div>
              <div className="text-xs text-vscode-descriptionForeground">({row.original.implName})</div>
            </div>
          </HoverCardTrigger>
          <HoverCardContent
            side="top"
            sideOffset={6}
            className="px-1 min-w-[400px] py-1 break-all border-0 border-none bg-vscode-input-background text-vscode-input-foreground overflow-y-scroll max-h-[500px] text-xs"
          >
            <JsonView
              enableClipboard={false}
              className="bg-[#1E1E1E] "
              theme="a11y"
              collapseStringsAfterLength={600}
              src={parseGlooObject({
                value: row.original.input,
              })}
            />
          </HoverCardContent>
        </HoverCard>
      )
    },
    accessorFn: (row) => `${row.testName}-${row.implName}`,
    id: 'testName-implName',
  },
  {
    id: 'status',
    accessorFn: (row) => ({
      status: row.status,
      partial_output: row.partial_output,
      error: row.output.error,
      render: row.output.parsed,
      raw: row.output.raw,
      url: row.url,
    }),
    cell: ({ getValue }) => {
      const val = getValue<{
        status: TestStatus; render?: string; error?: string; raw?: string; url?: string, partial_output: {
          raw?: string
          parsed?: string
        }
      }>()
      const [showJson, setShowJson] = useState(true)

      const raw_value = useMemo(() => {
        if (val.raw) {
          return val.raw
        }
        return val.partial_output.raw ?? ''
      }, [val.raw, val.partial_output.raw])

      const parsed_value = useMemo(() => {
        if (val.render) {
          return val.render
        }
        return val.partial_output.parsed ?? ''
      }, [val.render, val.partial_output.parsed])

      return (
        <div className="flex flex-col w-full p-0 text-xs">
          <div className="flex flex-row justify-between gap-x-1">
            <TestStatusIcon testStatus={val.status}>
              {val.url && (
                <VSCodeLink href={val.url}>
                  <ExternalLink className="w-4 h-4" />
                </VSCodeLink>
              )}
            </TestStatusIcon>
            {val.render ? (
              <div className="">
                <div className="flex items-center space-x-2">
                  <Label htmlFor="output" className="text-xs font-light text-vscode-descriptionForeground opacity-80">
                    Show Raw Output
                  </Label>
                  <Switch
                    id="output"
                    className="data-[state=checked]:bg-vscode-button-background data-[state=unchecked]:bg-vscode-input-background scale-75"
                    onCheckedChange={(e) => setShowJson(!e)}
                    checked={!showJson}
                  />
                </div>
              </div>
            ) : null}
          </div>

          {val.error && (
            <pre className="break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5">
              {pretty_error(val.error)}
            </pre>
          )}
          {parsed_value && (
            <pre className="break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5 relative bg-[#1E1E1E] text-white/90">
              {!showJson ? (
                raw_value
              ) : (
                <JsonView
                  enableClipboard={false}
                  className="bg-[#1E1E1E]"
                  theme="a11y"
                  collapseStringsAfterLength={600}
                  src={parseGlooObject({
                    value: pretty_stringify(parsed_value),
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
