'use client';
import { Label } from '@baml/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@baml/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@baml/ui/table'
import { useAtom, useAtomValue } from 'jotai'
import { Check, Copy, Play, Square } from 'lucide-react'
import * as React from 'react'

import { cn } from '@baml/ui/lib/utils'
import type { TestResponseData } from '../../../../../../sdk/interface'
import { ErrorBoundary } from 'react-error-boundary'
import { Button } from '@baml/ui/button'
import { TruncatedString } from '../../TruncatedString'
import { testcaseObjectAtom, TestState } from '../../../atoms'
import { type TestHistoryRun } from '../atoms'
import { useRunBamlTests } from '../test-runner'
import { getExplanation, getStatus, getTestStateResponse } from '../testStateUtils'
import { ResponseViewType, tabularViewConfigAtom } from './atoms'
import { MarkdownRenderer } from './MarkdownRenderer'
import { ParsedResponseRenderer } from './ParsedResponseRender'
import { useNavigation } from '../../../../../../sdk/hooks'
import { unifiedSelectionStateAtom } from '../../../../../../sdk/atoms/core.atoms'
import { TestStatus } from './TestStatus'
import { EnhancedErrorRenderer } from './EnhancedErrorRenderer'
import { useMemo, useCallback } from 'react'
import { vscode } from '../../../../vscode'
interface TabularViewProps {
  currentRun?: TestHistoryRun
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
  response: TestResponseData
}) => {
  const [copied, setCopied] = React.useState(false)

  const handleCopy = () => {
    const content =
      responseViewType === 'parsed'
        ? JSON.stringify(JSON.parse(response?.parsed_response?.value ?? ''), null, 2)
        : (response?.llm_response?.content ?? '')
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
  response: TestResponseData | undefined
  state: TestState
  responseViewType: ResponseViewType
}) => {
  const failureMessage = response?.failure_message

  return (
    <div className=''>
      {/* todo: render the failure if pretty or raw is selected. */}
      {responseViewType === 'parsed' && (
        <>
          <ParsedResponseRenderer response={getTestStateResponse(state) as TestResponseData | undefined} />

          {/* Don't show the explanation for now. */}
          {false && getExplanation(state) && (
            <div className='flex flex-col gap-2 mt-2 text-xs text-muted-foreground/80'>
              <div>BAML parser fixed the following issues:</div>
              <pre>{getExplanation(state)}</pre>
            </div>
          )}
        </>
      )}
      {responseViewType === 'pretty' && typeof getTestStateResponse(state) === 'object' && (
        <MarkdownRenderer source={(getTestStateResponse(state) as TestResponseData)?.llm_response?.content || ''} />
      )}
      {responseViewType === 'raw' && typeof getTestStateResponse(state) === 'object' && (
        <TruncatedString
          text={(getTestStateResponse(state) as TestResponseData)?.llm_response?.content || ''}
          maxLength={1500}
          headLength={600}
          tailLength={600}
          className='font-sans text-xs'
        />
      )}
    </div>
  )
}

export const TabularView: React.FC<TabularViewProps> = ({ currentRun }) => {
  const [config, setConfig] = useAtom(tabularViewConfigAtom)
  const { runTests: runBamlTests, cancelTests } = useRunBamlTests()
  const selection = useAtomValue(unifiedSelectionStateAtom)
  const navigate = useNavigation()

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

  const functionName = selection.mode === 'function' ? selection.functionName : selection.mode === 'workflow' ? selection.selectedNodeId : null;
  const testName = selection.mode === 'function' || selection.mode === 'workflow' ? selection.testName : selection.mode === 'loading' ? selection.intent.testName ?? null : null;

  const testAtom = useMemo(
    () => testcaseObjectAtom({ functionName: functionName ?? '', testcaseName: testName ?? '' }),
    [functionName, testName],
  )
  const tc = useAtomValue(testAtom)

  const selectedRowRef = React.useRef<HTMLTableRowElement>(null)



  React.useEffect(() => {
    if (functionName && testName && selectedRowRef.current) {
      selectedRowRef.current.scrollIntoView({
        behavior: 'smooth',
        block: 'nearest',
      })
    }
  }, [functionName, testName])


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
        <div className='flex items-center space-x-2'>
          <input
            type='checkbox'
            id='showDuration'
            checked={config.showDuration}
            onChange={() => toggleConfig('showDuration')}
            className='w-4 h-4 rounded opacity-80 text-primary focus:ring-primary'
          />
          <Label htmlFor='showDuration' className='text-muted-foreground/80'>
            Duration
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
            {config.showDuration && <TableHead className='w-[10%] py-1'>Duration</TableHead>}
          </TableRow>
        </TableHeader>
        <TableBody>
          {currentRun?.tests.map((test, index) => {
            const isSelected = functionName === test.functionName && testName === test.testName
            const isThisTestRunning = test.response.status === 'running'
            const testResponse = getTestStateResponse(test.response)
            const responseData = typeof testResponse === 'object' ? testResponse : undefined
            const rowKey = `${test.functionName}-${test.testName}`

            return (
              <TableRow
                key={rowKey}
                ref={isSelected ? selectedRowRef : undefined}
                className={cn(
                  'relative cursor-pointer transition-colors hover:bg-muted/70 ',
                  isSelected && 'border-purple-500/20 shadow-sm dark:border-purple-900/30 dark:bg-muted/90',
                )}
                onClick={() => {
                  // Navigate to test
                  navigate({
                    kind: 'test',
                    functionName: test.functionName,
                    testName: test.testName,
                    source: 'test-panel',
                    timestamp: Date.now(),
                  });
                }}
              >
                <TableCell className='px-1 py-1'>
                  <div className='flex flex-col items-center space-y-2'>
                    <Button
                      variant='ghost'
                      size='icon'
                      onClick={(e) => {
                        e.stopPropagation() // Prevent row selection when clicking the button
                        if (isThisTestRunning) {
                          cancelTests()
                        } else {
                          runBamlTests([
                            {
                              functionName: test.functionName,
                              testName: test.testName,
                            },
                          ])
                        }
                      }}
                      className='w-6 h-6'
                    >
                      {isThisTestRunning ? (
                        <Square className='w-4 h-4 fill-red-500 stroke-red-500' />
                      ) : (
                        <Play className='w-4 h-4 text-purple-400' />
                      )}
                    </Button>
                    <span
                      className='text-xs truncate whitespace-pre-wrap break-all cursor-pointer text-muted-foreground hover:text-primary'
                      onClick={(e) => {
                        e.stopPropagation()
                        if (!tc?.span) return
                        vscode.jumpToFile(tc.span);
                      }}
                    >
                      {test.testName}
                    </span>
                  </div>
                </TableCell>
                {config.showInputs && (
                  <TableCell className='py-1'>
                    <ErrorBoundary fallbackRender={() => <div>Error rendering input</div>}>
                      <TruncatedString
                        text={JSON.stringify(
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
                          }, {}) || {},
                          null,
                          2,
                        )}
                        maxLength={800}
                        headLength={300}
                        tailLength={300}
                        className="max-h-[400px] text-xs"
                      />
                    </ErrorBoundary>
                  </TableCell>
                )}
                <TableCell className='px-1 py-1'>
                  {/* <ScrollArea
                    className="relative max-h-[500px] flex-1"
                    type="always"
                  > */}
                  <ResponseContent
                    response={responseData}
                    state={test.response}
                    responseViewType={config.responseViewType}
                  />
                  {/* </ScrollArea> */}
                </TableCell>
                <TableCell className='px-1 py-1'>
                  <TestStatus status={test.response.status} finalState={getStatus(test.response)} />
                  {test.response.status === 'error' && (
                    <EnhancedErrorRenderer
                      errorMessage={test.response.message || 'Unknown error occurred'}
                      functionName={test.functionName}
                      testName={test.testName}
                      onRetry={() => runBamlTests([{ functionName: test.functionName, testName: test.testName }])}
                      className="text-xs"
                    />
                  )}
                </TableCell>
                {config.showModel && (
                  <TableCell className='px-1 py-1 whitespace-normal'>
                    {test.response.status === 'done' && test.response.response && (
                      <span className='text-xs text-muted-foreground'>
                        {test.response.response.llm_response?.model}
                      </span>
                    )}
                  </TableCell>
                )}
                {config.showDuration && (
                  <TableCell className='px-1 py-1 whitespace-normal'>
                    {test.response.status === 'done' && (
                      <span className='text-xs text-muted-foreground'>{test.response.latency_ms.toFixed(0)} ms</span>
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
