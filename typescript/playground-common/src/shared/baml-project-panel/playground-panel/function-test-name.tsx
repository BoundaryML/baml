import { ChevronRight, FlaskConical, FunctionSquare } from 'lucide-react'
import { vscode } from '../vscode'
import { functionObjectAtom, testcaseObjectAtom } from './atoms'
import { useAtomValue } from 'jotai'
import { useMemo } from 'react'

interface FunctionTestNameProps {
  functionName: string
  testName: string
  selected?: boolean
}

interface StringSpan {
  start: number
  end: number
  source_file: string
  value: string
}

export const FunctionTestName: React.FC<FunctionTestNameProps> = ({ functionName, testName, selected }) => {
  const functionAtom = useMemo(() => functionObjectAtom(functionName), [functionName])
  const testcaseAtom = useMemo(
    () => testcaseObjectAtom({ functionName, testcaseName: testName }),
    [functionName, testName],
  )
  const fn = useAtomValue(functionAtom)
  const tc = useAtomValue(testcaseAtom)
  const createSpan = (span: { start: number; end: number; file_path: string; start_line: number }) => ({
    start: span.start,
    end: span.end,
    source_file: span.file_path,
    value: `${span.file_path.split('/').pop() ?? '<file>.baml'}:${span.start_line + 1}`,
  })

  return (
    <div className={`flex w-full items-center space-x-1 text-xs ${selected ? '' : 'text-foreground/90'}`}>
      <div
        className='flex items-center cursor-pointer hover:text-primary'
        onClick={() => {
          if (fn?.span) {
            vscode.postMessage({ command: 'jumpToFile', span: createSpan(fn.span) })
          }
        }}
      >
        <FunctionSquare className='mr-1 w-3 h-3' />
        {functionName}
      </div>
      <ChevronRight className='w-3 h-3' />
      <div
        className='flex items-center cursor-pointer hover:text-primary'
        onClick={() => {
          if (tc?.span) {
            vscode.postMessage({ command: 'jumpToFile', span: createSpan(tc.span) })
          }
        }}
      >
        <FlaskConical className='mr-1 w-3 h-3' />
        {testName}
      </div>
    </div>
  )
}
