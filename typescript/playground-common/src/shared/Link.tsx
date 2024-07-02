import type { StringSpan } from '@baml/common'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import { cn } from '../lib/utils'
import { vscode } from '../utils/vscode'
import { File } from 'lucide-react'

const Link: React.FC<{ item: StringSpan; display?: string; className?: string }> = ({ item, display, className }) => (
  <VSCodeLink
    className={cn(className, 'text-vscode-foreground font-medium')}
    onClick={() => {
      vscode.postMessage({ command: 'jumpToFile', data: item })
    }}
  >
    <div className='flex flex-row items-center gap-1'>
      <File className='w-3' /> {display ?? item.value}
    </div>
  </VSCodeLink>
)

export default Link
