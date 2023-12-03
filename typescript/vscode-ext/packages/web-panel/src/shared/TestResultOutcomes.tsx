import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useSelections } from './hooks'
import { DataTable } from './TestResults/data-table'
import { columns } from './TestResults/columns'
import { VSCodePanelTab, VSCodePanelView, VSCodePanels } from '@vscode/webview-ui-toolkit/react'
import { useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'

const TestResultPanel = () => {
  const { test_results } = useSelections()

  const [selected, setSelection] = useState<string>('summary')

  return (
    <VSCodePanels
      activeid={`test-${selected}`}
      onChange={(e) => {
        const selected: string | undefined = (e.target as any)?.activetab?.id
        if (selected && selected.startsWith(`test-`)) {
          setSelection(selected.split('-', 2)[1])
        }
      }}
      className="h-full"
    >
      <VSCodePanelTab id={`test-summary`}>Summary</VSCodePanelTab>
      <VSCodePanelView id={`view-summary`}>
        <DataTable columns={columns} data={test_results} />
      </VSCodePanelView>
      <VSCodePanelTab id={`test-logs`}>Output</VSCodePanelTab>
      <VSCodePanelView id={`view-logs`}>
        <ScrollArea type="always" className="flex w-full h-full pr-3">
          <TestLogPanel />
        </ScrollArea>
      </VSCodePanelView>
    </VSCodePanels>
  )
}

const TestLogPanel = () => {
  const { test_log } = useSelections()

  return (
    <div className="h-full overflow-auto">
      <pre className="w-full whitespace-break-spaces">{test_log}</pre>
    </div>
  )
}

export default TestResultPanel
