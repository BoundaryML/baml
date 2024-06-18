import { Input } from '../components/ui/input'
import { selectedFunctionAtom } from '../baml_wasm_web/EventListener'
import { useAtomValue } from 'jotai'
import { Textarea } from '../components/ui/textarea'
import { Button } from '../components/ui/button'
import { Copy } from 'lucide-react'
import { useToast } from '../components/ui/use-toast'
import { useState } from 'react'

const FunctionTestSnippet: React.FC = () => {
  const selectedFunc = useAtomValue(selectedFunctionAtom)
  const [copied, setCopied] = useState(false)
  const snippet =
    selectedFunc?.test_snippet ??
    'test TestName {\n  functions [FunctionName]\n  args {\n   // Add your test case here\n  }\n}'

  const handleCopyToClipboard = () => {
    navigator.clipboard
      .writeText(snippet)
      .then(() => {
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
      })
      .catch((err) => {
        console.error('Failed to copy: ', err)
      })
  }

  return (
    <>
      <div className='relative flex flex-col w-full gap-1 h-fit'>
        <span className='font-semibold text-center'>Add this snippet to any .baml file to add a test</span>
        <div className='relative'>
          <Textarea
            readOnly
            className='h-[200px] p-2 overflow-y-auto font-mono text-xs rounded-sm bg-vscode-input-background border-0'
          >
            {snippet}
          </Textarea>
          <Button
            size='icon'
            className='absolute w-8 h-8 p-2 bg-transparent top-1 right-1 text-vscode-foreground hover:bg-vscode-editorHoverWidget-background'
            onClick={handleCopyToClipboard}
          >
            <Copy />
          </Button>
          {copied && (
            <span className='absolute p-1 text-xs text-green-500 rounded bg-vscode-tab-hoverBackground top-2 right-12'>
              Copied to Clipboard!
            </span>
          )}
        </div>
      </div>
    </>
  )
}

export default FunctionTestSnippet
