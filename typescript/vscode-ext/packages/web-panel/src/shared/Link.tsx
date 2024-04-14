import { vscode } from '../utils/vscode'
import { StringSpan } from '@baml/common'
import { VSCodeLink } from '@vscode/webview-ui-toolkit/react'

const Link: React.FC<{ item: StringSpan; display?: string }> = ({ item, display }) => (
  <VSCodeLink
    className="text-vscode-list-activeSelectionForeground"
    onClick={() => {
      vscode.postMessage({ command: 'jumpToFile', data: item })
    }}
  >
    {display ?? item?.value ?? ''}
  </VSCodeLink>
)

export default Link
