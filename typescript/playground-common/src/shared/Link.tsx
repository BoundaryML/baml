import { vscode } from '../utils/vscode'
import { StringSpan } from '@baml/common'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'
import { cn } from '../lib/utils'

const Link: React.FC<{ item: StringSpan; display?: string; className?: string }> = ({ item, display, className }) => (
  <VSCodeLink
    className={cn('text-vscode-foreground font-medium', className)}
    onClick={() => {
      vscode.postMessage({ command: 'jumpToFile', data: item })
    }}
  >
    {display ?? item.value}
  </VSCodeLink>
)

export default Link
