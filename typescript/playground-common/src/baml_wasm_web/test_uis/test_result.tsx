import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { VSCodeProgressRing } from '@vscode/webview-ui-toolkit/react'
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai'
import { PropsWithChildren } from 'react'
import {
  TestState,
  type TestStatusType,
  runningTestsAtom,
  statusCountAtom,
  testStatusAtom,
  DoneTestStatusType,
} from './testHooks'
import CustomErrorBoundary from '../../utils/ErrorFallback'
import { type TestStatus, type WasmTestResponse } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import JsonView from 'react18-json-view'
import 'react18-json-view/src/style.css'

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

const TestStatusIcon: React.FC<{ testRunStatus: TestStatusType; testStatus?: DoneTestStatusType }> = ({
  testRunStatus,
  testStatus,
}) => {
  return (
    <div className='text-vscode-descriptionForeground'>
      {
        {
          queued: 'Queued',
          running: <VSCodeProgressRing className='h-4' />,
          done: (
            <div className='flex flex-row items-center gap-1'>
              {testStatus && <TestStatusMessage testStatus={testStatus} />}
            </div>
          ),
          error: (
            <div className='flex flex-row items-center gap-1'>
              <div className='text-vscode-testing-iconFailed'>Unable to run</div>
            </div>
          ),
        }[testRunStatus]
      }
    </div>
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

const LLMTestResult: React.FC<{ test: WasmTestResponse; doneStatus: DoneTestStatusType }> = ({ test, doneStatus }) => {
  const failure = test.failure_message()
  const other = test.llm_response()
  const parsed = test.parsed_response()

  return (
    <div className='flex flex-col gap-1 w-full'>
      {failure && doneStatus !== 'parse_failed' && <div className='text-xs text-vscode-errorForeground'>{failure}</div>}
      {other && (
        <div className='text-xs text-vscode-descriptionForeground w-full'>
          <div>
            Took <b>{other.latency_ms.toString()}ms</b> using <b>{other.client}</b> (model: {other.model})
          </div>
          <div className='flex flex-row gap-2'>
            <div className='w-1/2 flex flex-col'>
              Raw LLM Response:
              <pre className='whitespace-pre-wrap bg-vscode-input-background py-2 px-1'>{other.content}</pre>
            </div>
            {(doneStatus === 'parse_failed' || parsed !== undefined) && (
              <div className='w-1/2 flex flex-col'>
                Parsed LLM Response:
                <div className='px-1 py-2'>
                  {failure && <pre className='text-xs text-vscode-errorForeground whitespace-pre-wrap'>{failure}</pre>}
                  {parsed !== undefined && (
                    <JsonView
                      enableClipboard={false}
                      theme='a11y'
                      collapseStringsAfterLength={200}
                      src={JSON.parse(parsed)}
                    />
                  )}
                </div>
              </div>
            )}
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
    <div className='flex flex-col'>
      <div className='flex flex-row items-center gap-2 text-xs'>
        <b>{name}</b>
        <TestStatusIcon
          testRunStatus={test.status}
          testStatus={test.status === 'done' ? test.response_status : undefined}
        />
      </div>
      {test.status === 'error' && <div className='text-xs text-vscode-errorForeground'>{test.message}</div>}
      {test.status === 'done' && (
        <div className='text-xs text-vscode-descriptionForeground'>
          <LLMTestResult test={test.response} doneStatus={test.response_status} />
        </div>
      )}
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
      className={`flex flex-row items-center gap-1 cursor-pointer ${
        selected ? '' : 'text-muted-foreground bg-vscode-button-backgroundHover'
      }`}
      onClick={onClick}
    >
      <span className='text-xs'>
        {name}: {count}
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

  return (
    <div className='flex flex-row gap-2'>
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

const TestResults: React.FC = () => {
  const testsRunning = useAtomValue(runningTestsAtom)
  return (
    <div className='flex flex-col gap-2'>
      <TestStatusBanner />
      <div className='flex flex-col gap-1 overflow-y-auto px-2'>
        {testsRunning.map((testName) => (
          <TestRow key={testName} name={testName} />
        ))}
      </div>
    </div>
  )
}

export default TestResults
