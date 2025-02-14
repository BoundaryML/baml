import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useAtom, useAtomValue } from 'jotai'
import { Check, Copy, Play } from 'lucide-react'
import * as React from 'react'

import { cn } from '@/lib/utils'
import { WasmFunctionResponse, WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web'
import { ErrorBoundary } from 'react-error-boundary'
import { Button } from '~/components/ui/button'
import { selectedItemAtom, testcaseObjectAtom, TestState } from '../../../atoms'
import { type TestHistoryRun } from '../atoms'
import { useRunTests } from '../test-runner'
import { getExplanation, getStatus, getTestStateResponse } from '../testStateUtils'
import { ResponseViewType, tabularViewConfigAtom } from './atoms'
import { MarkdownRenderer } from './MarkdownRenderer'
import { ParsedResponseRenderer } from './ParsedResponseRender'
import { TestStatus } from './TestStatus'
import { ScrollArea } from '~/components/ui/scroll-area'
import { vscode } from '@/shared/baml-project-panel/vscode'
import { useMemo } from 'react'
interface TabularViewProps {
  currentRun: TestHistoryRun
}

const testMarkdownWithJSXBlock = `
here is my answer:
\`\`\`jsx
const test = "test";

export default function Test() {
  return (
    <div>
      <div>Test</div>
    </div>
  );
}
\`\`\`
`

const CopyButton = ({
  responseViewType,
  response,
}: {
  responseViewType: ResponseViewType
  response: WasmTestResponse
}) => {
  const [copied, setCopied] = React.useState(false)

  const handleCopy = () => {
    const content =
      responseViewType === 'parsed'
        ? JSON.stringify(JSON.parse(response?.parsed_response()?.value ?? ''), null, 2)
        : (response?.llm_response()?.content ?? '')
    navigator.clipboard.writeText(content)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <Button
      variant='ghost'
      size='icon'
      className='absolute top-0 right-0 w-4 h-4 opacity-0 transition-opacity bg-muted group-hover:opacity-100'
      onClick={handleCopy}
    >
      {copied ? <Check className='w-4 h-4' /> : <Copy className='w-4 h-4' />}
    </Button>
  )
}

const ResponseContent = ({
  response,
  state,
  responseViewType,
}: {
  response: WasmTestResponse | WasmFunctionResponse | undefined
  state: TestState
  responseViewType: ResponseViewType
}) => {
  const failureMessage = response && 'failure_message' in response ? response.failure_message() : undefined

  return (
    <div className=''>
      {/* todo: render the failure if pretty or raw is selected. */}
      {responseViewType === 'parsed' && (
        <>
          <ParsedResponseRenderer response={getTestStateResponse(state)} />

          {getExplanation(state) && (
            <div className='flex flex-col gap-2 mt-2 text-xs text-muted-foreground/80'>
              <div>BAML parser fixed the following issues:</div>
              <pre>{getExplanation(state)}</pre>
            </div>
          )}
        </>
      )}
      {responseViewType === 'pretty' && (
        <MarkdownRenderer source={getTestStateResponse(state)?.llm_response()?.content || ''} />
      )}
      {responseViewType === 'raw' && (
        <pre className='font-sans text-xs whitespace-pre-wrap break-words'>
          {getTestStateResponse(state)?.llm_response()?.content}
        </pre>
      )}
    </div>
  )
}

export const TabularView: React.FC<TabularViewProps> = ({ currentRun }) => {
  const [config, setConfig] = useAtom(tabularViewConfigAtom)
  const { setRunningTests } = useRunTests() // Add runTest to the destructuring
  const [selectedItem, setSelectedItem] = useAtom(selectedItemAtom)

  const toggleConfig = (key: keyof typeof config) => {
    setConfig((prev) => ({
      ...prev,
      [key]: !prev[key],
    }))
  }

  const handleResponseTypeChange = (value: string) => {
    setConfig((prev) => ({
      ...prev,
      responseViewType: value as ResponseViewType,
    }))
  }

  const testAtom = useMemo(
    () => testcaseObjectAtom({ functionName: selectedItem?.[0] ?? '', testcaseName: selectedItem?.[1] ?? '' }),
    [selectedItem],
  )
  const tc = useAtomValue(testAtom)

  const createSpan = (span: { start: number; end: number; file_path: string; start_line: number }) => ({
    start: span.start,
    end: span.end,
    source_file: span.file_path,
    value: `${span.file_path.split('/').pop() ?? '<file>.baml'}:${span.start_line + 1}`,
  })

  const selectedRowRef = React.useRef<HTMLTableRowElement>(null)

  React.useEffect(() => {
    if (selectedItem && selectedRowRef.current) {
      selectedRowRef.current.scrollIntoView({
        behavior: 'smooth',
        block: 'nearest',
      })
    }
  }, [selectedItem])

  return (
    <div className='space-y-4'>
      <div className='flex items-center space-x-4'>
        <div className='flex items-center space-x-2'>
          <input
            type='checkbox'
            id='showInputs'
            checked={config.showInputs}
            onChange={() => toggleConfig('showInputs')}
            className='w-4 h-4 rounded opacity-80 text-primary focus:ring-primary'
          />
          <Label htmlFor='showInputs' className='text-muted-foreground/80'>
            Inputs
          </Label>
        </div>
        <div className='flex items-center space-x-2'>
          <input
            type='checkbox'
            id='showModel'
            checked={config.showModel}
            onChange={() => toggleConfig('showModel')}
            className='w-4 h-4 rounded opacity-80 text-primary focus:ring-primary'
          />
          <Label htmlFor='showModel' className='text-muted-foreground/80'>
            Model
          </Label>
        </div>
      </div>

      <Table className='w-full table-fixed'>
        <TableHeader>
          <TableRow>
            <TableHead className='w-[8%] py-1'>Test</TableHead>
            {config.showInputs && <TableHead className='w-[32%] py-1'>Inputs</TableHead>}
            <TableHead className={`${config.showModel ? 'w-[35%]' : 'w-[47%]'} py-1`}>
              <Select value={config.responseViewType} onValueChange={handleResponseTypeChange}>
                <SelectTrigger className='w-full text-left'>
                  <SelectValue placeholder='Response Type' />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value='parsed'>Parsed Response</SelectItem>
                  <SelectItem value='pretty'>Raw Response (markdown)</SelectItem>
                  <SelectItem value='raw'>Raw Response</SelectItem>
                </SelectContent>
              </Select>
            </TableHead>
            <TableHead className='w-[10%] px-1 py-1'>Status</TableHead>
            {config.showModel && <TableHead className='w-[10%] py-1'>Model</TableHead>}
          </TableRow>
        </TableHeader>
        <TableBody>
          {currentRun.tests.map((test, index) => {
            const isSelected = selectedItem?.[0] === test.functionName && selectedItem?.[1] === test.testName

            return (
              <TableRow
                key={index}
                ref={isSelected ? selectedRowRef : null}
                className={cn(
                  'relative cursor-pointer transition-colors hover:bg-muted/70',
                  isSelected && 'border-purple-500/20 shadow-sm dark:border-purple-900/30 dark:bg-muted/90',
                )}
                onClick={() => setSelectedItem(test.functionName, test.testName)}
              >
                <TableCell className='px-1 py-1'>
                  <div className='flex flex-col items-center space-y-2'>
                    <Button
                      variant='ghost'
                      size='icon'
                      onClick={(e) => {
                        e.stopPropagation() // Prevent row selection when clicking the button
                        setRunningTests([
                          {
                            functionName: test.functionName,
                            testName: test.testName,
                          },
                        ])
                      }}
                      className='w-6 h-6'
                    >
                      <Play className='w-4 h-4 text-purple-400' />
                    </Button>
                    <span
                      className='text-xs truncate whitespace-pre-wrap break-all cursor-pointer text-muted-foreground hover:text-primary'
                      onClick={(e) => {
                        e.stopPropagation()
                        if (!tc?.span) return
                        vscode.postMessage({ command: 'jumpToFile', span: createSpan(tc.span) })
                      }}
                    >
                      {test.testName}
                    </span>
                  </div>
                </TableCell>
                {config.showInputs && (
                  <TableCell className='py-1 whitespace-pre-wrap break-words'>
                    <ErrorBoundary fallbackRender={() => <div>Error rendering input</div>}>
                      <div className='max-h-[400px] overflow-auto text-xs'>
                        {test.input?.reduce((acc: Record<string, any>, input: { name?: string; value: any }) => {
                          let value = input.value
                          if (typeof value === 'string') {
                            try {
                              value = JSON.parse(value)
                            } catch {
                              // Keep original string if not valid JSON
                            }
                          }
                          if (input.name) {
                            acc[input.name] = value
                          }
                          return acc
                        }, {}) &&
                          JSON.stringify(
                            test.input?.reduce((acc: Record<string, any>, input: { name?: string; value: any }) => {
                              let value = input.value
                              if (typeof value === 'string') {
                                try {
                                  value = JSON.parse(value)
                                } catch {
                                  // Keep original string if not valid JSON
                                }
                              }
                              if (input.name) {
                                acc[input.name] = value
                              }
                              return acc
                            }, {}),
                            null,
                            2,
                          )}
                      </div>
                    </ErrorBoundary>
                  </TableCell>
                )}
                <TableCell className='px-1 py-1'>
                  {/* <ScrollArea
                    className="relative max-h-[500px] flex-1"
                    type="always"
                  > */}
                  <ResponseContent
                    response={getTestStateResponse(test.response)}
                    state={test.response}
                    responseViewType={config.responseViewType}
                  />
                  {/* </ScrollArea> */}
                </TableCell>
                <TableCell className='px-1 py-1'>
                  <TestStatus status={test.response.status} finalState={getStatus(test.response)} />
                  {test.response.status === 'error' && (
                    <div className='mt-1 text-xs text-red-500'>{test.response.message}</div>
                  )}
                </TableCell>
                {config.showModel && (
                  <TableCell className='px-1 py-1 whitespace-normal'>
                    {test.response.status === 'done' && test.response.response && (
                      <span className='text-xs text-muted-foreground'>
                        {test.response.response.llm_response()?.model}
                      </span>
                    )}
                  </TableCell>
                )}
              </TableRow>
            )
          })}
        </TableBody>
      </Table>
    </div>
  )
}
