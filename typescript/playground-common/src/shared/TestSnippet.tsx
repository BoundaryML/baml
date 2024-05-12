import { selectedFunctionAtom } from '../baml_wasm_web/EventListener'
import { useAtomValue } from 'jotai'

const FunctionTestSnippet: React.FC = () => {
  const selectedFunc = useAtomValue(selectedFunctionAtom)

  return (
    <div className='flex flex-col w-full items-center justify-center gap-1'>
      <span className='text-center'>Consider adding a test case to your file like:</span>
      <pre className='bg-vscode-input-background p-2 rounded-sm text-xs w-full'>
        {selectedFunc?.test_snippet ??
          'test TestName {\n  functions [FunctionName]\n  args {\n   // Add your test case here\n  }\n}'}
      </pre>
    </div>
  )
}

export default FunctionTestSnippet
