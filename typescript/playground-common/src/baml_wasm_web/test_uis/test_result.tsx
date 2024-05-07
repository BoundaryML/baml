import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { VSCodeProgressRing } from '@vscode/webview-ui-toolkit/react'
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai'
import { PropsWithChildren } from 'react'
import { TestState, TestStatusType, runningTestsAtom, statusCountAtom, testStatusAtom } from './testHooks'

const TestStatusIcon: React.FC<{ testStatus: TestStatusType }> = ({ testStatus }) => {
  return (
    <div className='text-vscode-descriptionForeground'>
      {
        {
          ['queued']: 'Queued',
          ['running']: <VSCodeProgressRing className='h-4' />,
          ['done']: (
            <div className='flex flex-row items-center gap-1'>
              <div className='text-vscode-testing-iconPassed'>Passed</div>
            </div>
          ),
          ['error']: (
            <div className='flex flex-row items-center gap-1'>
              <div className='text-vscode-testing-iconFailed'>Failed</div>
            </div>
          ),
        }[testStatus]
      }
    </div>
  )
}

const filterAtom = atom<Set<TestStatusType>>(new Set<TestStatusType>(['running', 'done', 'error']))

const TestRow: React.FC<{ name: string }> = ({ name }) => {
  const test = useAtomValue(testStatusAtom(name))
  const filter = useAtomValue(filterAtom)

  if (filter.size > 0 && !filter.has(test.status)) {
    return null
  }

  return (
    <div className='flex flex-col'>
      <div className='flex flex-row items-center gap-2 text-xs'>
        <b>{name}</b>
        <TestStatusIcon testStatus={test.status} />
      </div>
      {test.status === 'error' && <div className='text-xs text-vscode-errorForeground'>{test.message}</div>}
      {test.status === 'done' && <div className='text-xs text-vscode-descriptionForeground'>{test.response}</div>}
    </div>
  )
}

const TestStatusBanner: React.FC<{ statusCounts: { [key in TestStatusType]: number } }> = ({ statusCounts }) => {
  const statuses: TestStatusType[] = ['queued', 'running', 'done', 'error']
  const [filter, setFilter] = useAtom(filterAtom)

  const toggleFilter = (status: TestStatusType) => {
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
      {statuses
        .map((s): [TestStatusType, number] => [s, statusCounts[s] ?? 0])
        .map(([status, count]) => (
          <Badge
            key={status}
            className={`flex flex-row items-center gap-1  cursor-pointer ${
              filter.has(status) ? '' : 'text-muted-foreground bg-vscode-button-backgroundHover'
            }`}
            onClick={() => toggleFilter(status)}
          >
            <span className='text-xs'>
              {status}: {count}
            </span>
          </Badge>
        ))}
    </div>
  )
}

const TestResults: React.FC = () => {
  const testsRunning = useAtomValue(runningTestsAtom)
  const statusCounts = useAtomValue(statusCountAtom)
  return (
    <div className='flex flex-col gap-1'>
      <TestStatusBanner statusCounts={statusCounts} />
      {testsRunning.map((testName) => (
        <TestRow key={testName} name={testName} />
      ))}
    </div>
  )
}

export default TestResults
