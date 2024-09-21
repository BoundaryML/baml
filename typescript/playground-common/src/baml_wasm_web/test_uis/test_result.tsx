import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { VSCodeButton, VSCodeLink, VSCodeProgressRing, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai'
import React, { PropsWithChildren, useEffect, useMemo, useState, useCallback } from 'react'
import {
  TestState,
  type TestStatusType,
  runningTestsAtom,
  statusCountAtom,
  testStatusAtom,
  DoneTestStatusType,
  useRunHooks,
  showTestsAtom,
  showClientGraphAtom,
} from './testHooks'
import CustomErrorBoundary from '../../utils/ErrorFallback'
import {
  type WasmTestCase,
  type TestStatus,
  type WasmTestResponse,
  type WasmParsedTestResponse,
  WasmFunctionResponse,
} from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import JsonView from 'react18-json-view'
import clsx from 'clsx'
import { Check, Copy, FilterIcon, InfoIcon, Link2Icon, PlayIcon, PlusIcon } from 'lucide-react'
import { currentClientsAtom, selectedFunctionAtom, selectedTestCaseAtom } from '../EventListener'
import { Dialog, DialogContent, DialogTrigger } from '@/components/ui/dialog'
import FunctionTestSnippet from '../../shared/TestSnippet'
import { Tooltip, TooltipContent } from '../../components/ui/tooltip'
import { TooltipTrigger } from '../../components/ui/tooltip'
import { orchestration_nodes, orchIndexAtom } from '../../baml_wasm_web/EventListener'
import {
  ReactFlow,
  addEdge,
  Background,
  useNodesState,
  useEdgesState,
  MiniMap,
  MarkerType,
  Connection,
} from '@xyflow/react'
import { Sparkles } from 'lucide-react'
import { ErrorBoundary } from 'react-error-boundary'

const TestStatusMessage: React.FC<{ testStatus: DoneTestStatusType }> = ({ testStatus }) => {
  switch (testStatus) {
    case 'passed':
      return <div className='text-vscode-testing-iconPassed'>Passed</div>
    case 'llm_failed':
      return <div className='text-vscode-testing-iconFailed'>LLM Failed</div>
    case 'parse_failed':
      return <div className='text-vscode-testing-iconFailed'>Parse Failed</div>
    case 'error':
      return <div className='text-vscode-testing-iconFailed'>Unable to run</div>
  }
}

const TestStatusIcon: React.FC<{
  testRunStatus: TestStatusType
  testStatus?: DoneTestStatusType
  traceUrl?: string
}> = ({ testRunStatus, testStatus, traceUrl }) => {
  const testStatusIcon = (
    <div className='text-vscode-descriptionForeground'>
      {
        {
          queued: 'Queued',
          running: <VSCodeProgressRing className='h-4' />,
          done: (
            <div className='flex flex-row gap-1 items-center'>
              {testStatus && <TestStatusMessage testStatus={testStatus} />}
              {traceUrl && <Link2Icon className='inline w-3 h-3 text-center hover:underline icon-link' />}
            </div>
          ),
          error: (
            <div className='flex flex-row gap-1 items-center'>
              <div className='text-vscode-testing-iconFailed'>Unable to run</div>
            </div>
          ),
        }[testRunStatus]
      }
    </div>
  )

  return traceUrl ? (
    <a className='hover:underline' href={traceUrl}>
      {testStatusIcon}
    </a>
  ) : (
    testStatusIcon
  )
}

type FilterValues = 'queued' | 'running' | 'error' | 'llm_failed' | 'parse_failed' | 'passed'
const filterAtom = atom(new Set<FilterValues>(['running', 'error', 'llm_failed', 'parse_failed', 'passed']))

const checkFilter = (filter: Set<FilterValues>, status: TestStatusType, test_status?: DoneTestStatusType) => {
  if (filter.size === 0) {
    return true
  }

  if (status === 'done') {
    if (test_status === undefined) {
      return false
    }
    return filter.has(test_status)
  }

  return filter.has(status)
}

const copyToClipboard = (text: string, setCopied: React.Dispatch<React.SetStateAction<boolean>>) => {
  navigator.clipboard.writeText(text)
  setCopied(true)
  setTimeout(() => setCopied(false), 2000)
}

const ParsedTestResult: React.FC<{ doneStatus: string; parsed?: WasmParsedTestResponse; failure?: string }> = ({
  doneStatus,
  parsed,
  failure,
}) => {
  const [copiedParsed, setCopiedParsed] = useState(false)
  const [showExplanation, setShowExplanation] = useState(false)

  const sorted_parsed = parsed ? JSON.parse(parsed.value) : undefined

  if (doneStatus === 'parse_failed' || parsed !== undefined) {
    return (
      <div className='flex relative flex-col'>
        <div className='flex flex-row '>
          Parsed LLM Response
          <Tooltip delayDuration={100}>
            <TooltipTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='inline h-2 w-2 pl-1'
                onClick={() => {
                  if (parsed?.explanation) {
                    setShowExplanation(!showExplanation)
                  }
                }}
                aria-label='Toggle information'
              >
                {showExplanation ? (
                  <div className='text-xs text-white whitespace-prewrap'>
                    <InfoIcon className='h-4 w-4 inline-block align-top' />
                    &nbsp;Showing Explanation
                  </div>
                ) : (
                  <InfoIcon className='h-4 w-4' />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {(() => {
                if (showExplanation) {
                  return 'Showing parsing explanation. Click to show the parsed response.'
                }
                if (parsed?.explanation) {
                  return 'Click to show parsing explanation'
                }
                return 'Parsing succeeded'
              })()}
            </TooltipContent>
          </Tooltip>
        </div>
        <div className='relative px-1 py-2'>
          {showExplanation && parsed && parsed.explanation ? (
            <>
              <JsonView
                enableClipboard={false}
                className='bg-[#1E1E1E] px-1 py-1 rounded-sm'
                theme='a11y'
                collapseStringsAfterLength={200}
                matchesURL
                src={JSON.parse(parsed.explanation)}
              />
              <div className='flex absolute right-2 top-3 items-center'>
                <button
                  className='text-vscode-descriptionForeground hover:text-vscode-foreground'
                  onClick={() => copyToClipboard(JSON.stringify(sorted_parsed, null, 2), setCopiedParsed)}
                >
                  {copiedParsed ? <Check size={16} /> : <Copy size={16} />}
                </button>
                {copiedParsed && <span className='ml-1 text-xs text-vscode-foreground'>Copied</span>}
              </div>
            </>
          ) : (
            <>
              {failure && <pre className='text-xs whitespace-pre-wrap text-vscode-errorForeground'>{failure}</pre>}
              {parsed !== undefined && (
                <>
                  <JsonView
                    enableClipboard={false}
                    className='bg-[#1E1E1E] px-1 py-1 rounded-sm'
                    theme='a11y'
                    collapseStringsAfterLength={200}
                    matchesURL
                    src={sorted_parsed}
                  />
                  <div className='flex absolute right-2 top-3 items-center'>
                    <button
                      className='text-vscode-descriptionForeground hover:text-vscode-foreground'
                      onClick={() => copyToClipboard(JSON.stringify(sorted_parsed, null, 2), setCopiedParsed)}
                    >
                      {copiedParsed ? <Check size={16} /> : <Copy size={16} />}
                    </button>
                    {copiedParsed && <span className='ml-1 text-xs text-vscode-foreground'>Copied</span>}
                  </div>
                </>
              )}
            </>
          )}
        </div>
      </div>
    )
  } else {
    return <></>
  }
}

const LLMTestResult: React.FC<{ test: WasmTestResponse; doneStatus: DoneTestStatusType; testLatency: number }> = ({
  test,
  doneStatus,
  testLatency,
}) => {
  const [copiedRaw, setCopiedRaw] = useState(false)

  const failure = test.failure_message()
  const llm_response = test.llm_response()
  const llm_failure = test.llm_failure()
  const parsed = test.parsed_response()

  const latencyMs = llm_response?.latency_ms ?? llm_failure?.latency_ms
  const client = llm_response?.client_name() ?? llm_failure?.client_name()
  const model = llm_response?.model ?? llm_failure?.model
  const bamlOverheadLatency = testLatency - (latencyMs ? Number(latencyMs) : 0)
  console.log('llm response', {
    latency_ms: llm_response?.latency_ms,
    output_tokens: llm_response?.output_tokens,
    model: llm_response?.model,
    stop_reason: llm_response?.stop_reason,
  })

  const details = [
    [llm_response?.model, `${llm_response?.model}`],
    [
      llm_response?.input_tokens && llm_response?.output_tokens,
      `${llm_response?.input_tokens} input tokens → ${llm_response?.output_tokens} output tokens`,
    ],
  ]
    .filter((x) => x[0] !== undefined)
    .map((x) => x[1])

  const stopReasonText = llm_response?.stop_reason ? ` |  StopReason: ${llm_response?.stop_reason} | ` : ''
  const detailsText = details.length > 0 ? `${stopReasonText}  (${details.join(', ')})` : ''

  return (
    <div className='flex flex-col gap-1 w-full'>
      {failure !== undefined &&
        !(doneStatus === 'parse_failed' || (doneStatus === 'llm_failed' && (llm_response || llm_failure))) && (
          <div className='text-xs whitespace-pre-wrap text-vscode-errorForeground'>
            {failure || '<no failure message>'}
          </div>
        )}
      {(llm_response || llm_failure) && (
        <div className='w-full text-xs text-vscode-descriptionForeground'>
          <div>
            <b>{latencyMs?.toString()}ms</b> using <b>{client}</b>
            {detailsText}
            {latencyMs !== undefined && bamlOverheadLatency > 0 && <> (+ {bamlOverheadLatency}ms for BAML)</>}
          </div>
          <div className='grid grid-cols-2 gap-2'>
            <div className='flex flex-col'>
              {llm_response?.output_tokens === undefined
                ? 'Raw LLM Response'
                : `Raw LLM Response (${llm_response?.output_tokens} tokens)`}
              <div className='px-1 py-2'>
                {llm_response && (
                  <pre className='px-1 py-2 whitespace-pre-wrap rounded-sm bg-vscode-input-background max-h-[300px] overflow-y-auto relative pr-2'>
                    {llm_response.content}
                    <div className='flex absolute top-1 right-1 items-center'>
                      <button
                        className='text-vscode-foreground hover:text-vscode-button-foreground'
                        onClick={() => copyToClipboard(llm_response.content, setCopiedRaw)}
                      >
                        {copiedRaw ? <Check size={16} /> : <Copy size={16} />}
                      </button>
                      {copiedRaw && <span className='ml-1 text-xs text-vscode-foreground'>Copied</span>}
                    </div>
                  </pre>
                )}
                {llm_failure && (
                  <pre className='text-xs whitespace-pre-wrap text-vscode-errorForeground'>
                    <b>{llm_failure.code}</b>
                    <br />
                    {llm_failure.message}
                    <br />
                    <br />
                    {(window as any).next?.version ? (
                      <>Check the browser's network tab for more details</>
                    ) : (
                      <>
                        {
                          'Check the webview network tab for more details. Command Palette -> Open webview developer tools. '
                        }
                      </>
                    )}
                    <br />
                  </pre>
                )}
              </div>
            </div>
            <ParsedTestResult doneStatus={doneStatus} parsed={parsed} failure={failure} />
          </div>
        </div>
      )}
    </div>
  )
}

const LLMFunctionResult: React.FC<{ test: WasmFunctionResponse }> = ({ test }) => {
  const llm_response = test.llm_response()
  const llm_failure = test.llm_failure()
  const parsed = test.parsed_response()
  const sorted_parsed = parsed ? JSON.parse(parsed) : undefined

  const latencyMs = llm_response?.latency_ms ?? llm_failure?.latency_ms
  const client = llm_response?.client_name() ?? llm_failure?.client_name()
  const model = llm_response?.model ?? llm_failure?.model

  return (
    <div className='flex flex-col gap-1 w-full'>
      {(llm_response || llm_failure) && (
        <div className='w-full text-xs text-vscode-descriptionForeground'>
          <div>
            <b>{latencyMs?.toString()}ms</b> using <b>{client}</b> {model && <>(model: {model})</>}{' '}
          </div>
          <div className='grid grid-cols-2 gap-2'>
            <div className='flex flex-col'>
              Raw LLM Response:
              <div className='px-1 py-2'>
                {llm_response && (
                  <pre className='px-1 py-2 whitespace-pre-wrap rounded-sm bg-vscode-input-background max-h-[200px] overflow-y-auto'>
                    {llm_response.content}
                  </pre>
                )}
                {llm_failure && (
                  <pre className='text-xs whitespace-pre-wrap text-vscode-errorForeground'>
                    <b>{llm_failure.code}</b>
                    <br />
                    {llm_failure.message}
                    <br />
                    <br />
                    {(window as any).next?.version ? (
                      <>Check the browser's network tab for more details</>
                    ) : (
                      <>
                        {
                          'Check the webview network tab for more details. Command Palette -> Open webview developer tools. '
                        }
                      </>
                    )}
                    <br />
                  </pre>
                )}
              </div>
            </div>
            <div className='flex flex-col'>
              Parsed LLM Response:
              <div className='px-1 py-2'>
                {parsed !== undefined ? (
                  <JsonView
                    enableClipboard={false}
                    className='bg-[#1E1E1E] px-1 py-1 rounded-sm'
                    theme='a11y'
                    collapseStringsAfterLength={200}
                    matchesURL
                    src={sorted_parsed}
                  />
                ) : (
                  <pre className='text-xs whitespace-pre-wrap text-vscode-errorForeground'>
                    Waiting for a parsable-reply
                  </pre>
                )}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

const TestRow: React.FC<{ name: string }> = ({ name }) => {
  const test = useAtomValue(testStatusAtom(name))
  const filter = useAtomValue(filterAtom)

  if (!checkFilter(filter, test.status, test.status === 'done' ? test.response_status : undefined)) {
    return null
  }

  return (
    <div className='flex flex-row gap-2 items-start group'>
      <TestCaseActions testName={name} />
      <div className='flex flex-col'>
        <div className='flex flex-row gap-2 items-center text-xs'>
          <b>{name}</b>
          <TestStatusIcon
            testRunStatus={test.status}
            testStatus={test.status === 'done' ? test.response_status : undefined}
            traceUrl={test.status === 'done' ? test.response.trace_url() : undefined}
          />
        </div>
        {test.status === 'error' && <div className='text-xs text-vscode-errorForeground'>{test.message}</div>}
        {test.status === 'running' && test.response && (
          <div className='text-xs text-vscode-descriptionForeground'>
            <LLMFunctionResult test={test.response} />
          </div>
        )}
        {test.status === 'done' && (
          <div className='text-xs text-vscode-descriptionForeground'>
            <LLMTestResult test={test.response} doneStatus={test.response_status} testLatency={test.latency_ms} />
          </div>
        )}
      </div>
    </div>
  )
}

const FilterButton: React.FC<{ selected: boolean; name: string; count: number; onClick: () => void }> = ({
  selected,
  name,
  count,
  onClick,
}) => {
  return (
    <Badge
      className={`flex flex-row items-center gap-1 cursor-pointer bg-vscode-list-inactiveSelectionBackground ${
        selected
          ? 'text-vscode-list-activeSelectionForeground'
          : 'text-vscode-list-inactiveSelectionForeground bg-vscode-button-backgroundHover'
      }`}
      onClick={onClick}
    >
      <span className='text-xs'>
        {name} {count}
      </span>
    </Badge>
  )
}

const TestStatusBanner: React.FC = () => {
  const statusCounts = useAtomValue(statusCountAtom)

  const [filter, setFilter] = useAtom(filterAtom)

  const toggleFilter = (status: FilterValues) => {
    setFilter((prevFilter) => {
      const newFilter = new Set(prevFilter)
      if (newFilter.has(status)) {
        newFilter.delete(status)
      } else {
        newFilter.add(status)
      }
      return newFilter
    })
  }

  const isNextJS = (window as any).next?.version!
  if (isNextJS) {
    // simplify UI in promptfiddle
    return null
  }

  return (
    <div className='flex flex-row flex-wrap gap-2 items-center'>
      <FilterIcon size={16} />
      <FilterButton
        selected={filter.has('queued')}
        name='Queued'
        count={statusCounts.queued}
        onClick={() => toggleFilter('queued')}
      />
      <FilterButton
        selected={filter.has('running')}
        name='Running'
        count={statusCounts.running}
        onClick={() => toggleFilter('running')}
      />
      <FilterButton
        selected={filter.has('error')}
        name='Error'
        count={statusCounts.error + statusCounts.done.error}
        onClick={() => toggleFilter('error')}
      />
      <FilterButton
        selected={filter.has('llm_failed')}
        name='LLM Failed'
        count={statusCounts.done.llm_failed}
        onClick={() => toggleFilter('llm_failed')}
      />
      <FilterButton
        selected={filter.has('parse_failed')}
        name='Parse Failed'
        count={statusCounts.done.parse_failed}
        onClick={() => toggleFilter('parse_failed')}
      />
      <FilterButton
        selected={filter.has('passed')}
        name='Passed'
        count={statusCounts.done.passed}
        onClick={() => toggleFilter('passed')}
      />
    </div>
  )
}

interface RenderEdge {
  id: string
  source: string
  target: string
}

interface RenderNode {
  id: string
  data: { label: string; orch_index: number }
  position: { x: number; y: number }
  style?: { backgroundColor: string; width?: number; height?: number }
  parentId?: string
  extent?: 'parent' | undefined // Update extent type
}

const ClientGraph: React.FC = () => {
  const graph = useAtomValue(orchestration_nodes)
  const [orchIndex, setOrchIndex] = useAtom(orchIndexAtom)

  const renderNodes: RenderNode[] = useMemo(
    () =>
      graph.nodes.map((node) => ({
        id: node.gid,
        data: {
          label: node.client_name ?? 'no name for this node',
          orch_index: node.orch_index !== undefined ? node.orch_index : -1,
        },
        position: { x: node.Position?.x ?? 0, y: node.Position?.y ?? 0 },
        style: {
          backgroundColor: 'rgba(255, 0, 255, 0.2)',
          width: node.Dimension?.width,
          height: node.Dimension?.height,
          outline: orchIndex === node.orch_index ? '1px solid white' : '',
        },
        parentId: node.parentGid,
        extent: 'parent',
      })),
    [graph.nodes, orchIndex],
  )

  const renderEdges: RenderEdge[] = useMemo(
    () =>
      graph.edges.map((edge, idx) => ({
        id: idx.toString(),
        source: edge.from_node,
        target: edge.to_node,
        animated: true,
        type: 'smoothstep',
        markerEnd: {
          type: MarkerType.ArrowClosed,
        },
        label: edge.weight !== undefined ? `⏰ ${edge.weight} ms ` : '',
      })),
    [graph.edges],
  )

  const [flowNodes, setFlowNodes, onNodesChange] = useNodesState(renderNodes)
  const [flowEdges, setFlowEdges, onEdgesChange] = useEdgesState(renderEdges)

  // const onConnect = useCallback((connection: Connection) => {
  //   setFlowEdges((eds) => addEdge(connection, eds))
  // }, [])

  // Set default selected node

  // Synchronize flowNodes and flowEdges with nodes and edges
  useEffect(() => {
    setFlowNodes(renderNodes)
    setFlowEdges(renderEdges)
  }, [renderNodes, renderEdges])

  const onNodeClick = (event: React.MouseEvent, node: any) => {
    if (node.data.orch_index != -1) {
      setOrchIndex(node.data.orch_index)
    }
  }

  const styles: React.CSSProperties = {
    whiteSpace: 'normal',
    wordWrap: 'break-word',
    overflowWrap: 'break-word',
  }

  return (
    <div style={{ height: '100vh', width: '100%' }}>
      <ReactFlow
        style={styles}
        nodes={flowNodes}
        edges={flowEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={onNodeClick}
        fitView
        edgesFocusable={false}
        nodesDraggable={false}
        nodesConnectable={false}
        nodesFocusable={false}
      />
    </div>
  )
}

const TestResults: React.FC = () => {
  const selectedFunction = useAtomValue(selectedFunctionAtom)
  const [showTests, setShowTests] = useAtom(showTestsAtom)
  const [showClientGraph, setClientGraph] = useAtom(showClientGraphAtom)

  // reset the tab when switching funcs
  useEffect(() => {
    setShowTests(false)
  }, [selectedFunction?.name])
  const isNextJs = (window as any).next?.version

  return (
    <div className='flex flex-col gap-2 px-1 w-full'>
      <div className='flex flex-row gap-2 items-center'>
        <Badge
          className={clsx(
            'cursor-pointer hover:bg-vscode-tab-activeBackground',
            showTests || showClientGraph
              ? 'bg-transparent text-vscode-foreground'
              : 'bg-vscode-tab-activeBackground text-vscode-tab-activeForeground underline',
          )}
          onClick={() => {
            setShowTests(false)
            setClientGraph(false)
          }}
        >
          All Tests
        </Badge>
        <Badge
          className={clsx(
            'cursor-pointer hover:bg-vscode-tab-activeBackground',
            showTests && !showClientGraph
              ? 'bg-vscode-tab-activeBackground text-vscode-tab-activeForeground underline'
              : 'bg-transparent text-vscode-foreground',
          )}
          onClick={() => {
            setShowTests(true)
            setClientGraph(false)
          }}
        >
          Test Results
        </Badge>
        <Badge
          className={clsx(
            'cursor-pointer hover:bg-vscode-tab-activeBackground',
            showClientGraph
              ? 'underline bg-vscode-tab-activeBackground text-vscode-tab-activeForeground'
              : 'bg-transparent text-vscode-foreground',
          )}
          onClick={() => {
            setClientGraph(true)
          }}
        >
          Client Graph ✨
        </Badge>
      </div>

      <CustomErrorBoundary>
        {showClientGraph ? <ClientGraph /> : showTests ? <TestResultContent /> : <TestCaseList />}
      </CustomErrorBoundary>
    </div>
  )
}

const TestCaseActions: React.FC<{ testName: string }> = ({ testName }) => {
  const [selectedTestCase, setSelectedTestCase] = useAtom(selectedTestCaseAtom)

  const { isRunning, run } = useRunHooks()

  return (
    <div className='flex flex-col gap-1 pt-2'>
      <Button
        variant={'ghost'}
        size={'icon'}
        className='p-1 rounded-md w-fit h-fit bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground'
        disabled={isRunning}
        onClick={() => {
          run([testName])
        }}
      >
        <PlayIcon size={10} />
      </Button>
      {/* {selectedTestCase?.name === testName ? (
        <Button
          variant={'ghost'}
          size={'icon'}
          className='flex p-1 rounded-md w-fit h-fit bg-vscode-button-background text-vscode-button-foreground'
          disabled
        >
          <Pin size={10} />
        </Button>
      ) : (
        <Button
          variant={'ghost'}
          size={'icon'}
          className='hidden p-1 rounded-md w-fit h-fit hover:bg-vscode-button-background hover:text-vscode-button-foreground group-hover:flex'
          onClick={() => {
            setSelectedTestCase(testName)
          }}
        >
          <Pin size={10} />
        </Button>
      )} */}
    </div>
  )
}

const NewTestCaseDialog: React.FC = () => {
  const [show, setShow] = useState(false)
  const selectedFunction = useAtomValue(selectedFunctionAtom)

  if (!selectedFunction) {
    return null
  }

  return (
    <Dialog open={show} onOpenChange={setShow}>
      <DialogTrigger asChild={true}>
        <Button variant='outline' className='p-1 text-xs truncate w-fit h-fit border-vscode-textSeparator-foreground'>
          <PlusIcon size={16} /> New Test
        </Button>
      </DialogTrigger>
      <DialogContent className='max-h-screen min-h-[50%] overflow-y-auto bg-purple-400 bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip'>
        <div className='flex flex-col w-full h-full'>
          <FunctionTestSnippet />
        </div>
      </DialogContent>
    </Dialog>
  )
}

const TestCaseList: React.FC = () => {
  const allTestCases = useAtomValue(selectedFunctionAtom)?.test_cases ?? []
  const [filter, setFilter] = useState('')
  const testCases = useMemo(() => {
    return allTestCases.filter((t) => t.name.includes(filter) || t.inputs.some((i) => i.value?.includes(filter)))
  }, [allTestCases, filter])

  const [selectedTestCase, setSelectedTestCase] = useAtom(selectedTestCaseAtom)

  const { isRunning, run } = useRunHooks()

  return (
    <div className='flex flex-col gap-2 px-2 w-full h-full'>
      <div className='flex flex-wrap gap-2 items-start items-center h-fit'>
        <div className='flex flex-col'>
          <div className='flex flex-wrap gap-2 items-center'>
            <FilterIcon size={16} />
            <VSCodeTextField
              placeholder='Filter test cases'
              className='w-32 shrink'
              value={filter}
              onInput={(e) => {
                setFilter((e as React.FormEvent<HTMLInputElement>).currentTarget.value)
              }}
            />
          </div>

          {filter && (
            <div className='text-xs text-muted-foreground'>{allTestCases.length - testCases.length} filtered out</div>
          )}
        </div>
        {isRunning ? (
          <VSCodeButton className='bg-vscode-statusBarItem-errorBackground' disabled onClick={() => {}}>
            Cancel Not Supported
          </VSCodeButton>
        ) : (
          <>
            <Button
              className='px-1 py-1 text-xs whitespace-nowrap bg-red-500 rounded-sm h-fit bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground'
              disabled={testCases.length === 0}
              onClick={() => {
                run(testCases.map((t) => t.name))
              }}
            >
              <div className='flex flex-row gap-1 items-center'>
                <PlayIcon size={10} />
                Run {filter ? testCases.length : 'all'} tests
              </div>
            </Button>
          </>
        )}
        <NewTestCaseDialog />
      </div>
      <hr className='w-full border-muted-foreground' />
      <div className='flex overflow-y-auto flex-col gap-1 h-full'>
        {testCases.map((test) => (
          <div key={test.name} className='flex flex-row gap-2 items-start group'>
            <TestCaseActions testName={test.name} />
            <Tooltip>
              <TooltipTrigger asChild>
                <div
                  className={clsx(
                    'flex flex-col gap-1 p-2 w-full',
                    selectedTestCase?.name !== test.name
                      ? 'cursor-pointer hover:bg-vscode-input-background'
                      : 'border-vscode-input-background border rounded-sm bg-vscode-input-background',
                  )}
                  onClick={
                    selectedTestCase?.name === test.name
                      ? undefined
                      : () => {
                          setSelectedTestCase(test.name)
                        }
                  }
                >
                  <div className='text-xs'>{test.name}</div>
                  <TestCaseCard test_case={test} />
                </div>
              </TooltipTrigger>
              <TooltipContent className=''>Render this test in the prompt</TooltipContent>
            </Tooltip>
          </div>
        ))}
      </div>
    </div>
  )
}

const TestCaseCard: React.FC<{ test_case: WasmTestCase }> = ({ test_case }) => {
  return (
    <div className='flex flex-col gap-2 max-w-full text-xs text-left truncate text-vscode-descriptionForeground'>
      <div className='whitespace-pre-wrap break-all'>
        <div className='flex flex-col'>
          {test_case.inputs.map((input) => (
            <div key={input.name}>
              <b>{input.name}:</b>
              {input.value !== undefined && (
                <JsonView
                  enableClipboard={false}
                  className='bg-[#1E1E1E] px-1 py-1 rounded-sm'
                  theme='a11y'
                  collapseStringsAfterLength={150}
                  collapseObjectsAfterLength={4}
                  matchesURL
                  src={JSON.parse(input.value)}
                />
              )}
              {input.error && (
                <pre className='break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5'>
                  {input.error}
                </pre>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

const TestResultContent: React.FC = () => {
  const testsRunning = useAtomValue(runningTestsAtom)
  return (
    <div className='flex flex-col gap-2 px-2 w-full h-full'>
      <TestStatusBanner />
      <hr className='border-muted-foreground' />
      <div className='flex overflow-y-auto flex-col gap-1 w-full h-full'>
        {testsRunning.map((testName) => (
          <TestRow key={testName} name={testName} />
        ))}
      </div>
    </div>
  )
}

export default TestResults
