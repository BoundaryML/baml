import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useSelections } from './hooks'
import { DataTable } from './TestResults/data-table'
import { columns } from './TestResults/columns'
import {
  VSCodeLink,
  VSCodePanelTab,
  VSCodePanelView,
  VSCodePanels,
  VSCodeProgressRing,
} from '@vscode/webview-ui-toolkit/react'
import { useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Button } from '@/components/ui/button'
import Link from './Link'
import { AlertTriangle, ExternalLink, FileWarningIcon } from 'lucide-react'

const TestResultPanel = () => {
  const { test_results, test_result_url, test_result_exit_status } = useSelections()

  const [selected, setSelection] = useState<string>('summary')

  if (!test_results) {
    return (
      <>
        <div className="flex flex-col items-center justify-center">
          <div className="flex flex-col items-center justify-center space-y-2">
            <div className="text-2xl font-semibold">No test results for this function</div>
            <div className="text-sm font-light">Run tests to see results</div>
          </div>
        </div>
      </>
    )
  }

  return (
    <>
      {test_result_url && (
        <div className="flex flex-row justify-center bg-vscode-menu-background items-center w-full">
          <VSCodeLink href={test_result_url.url}>
            <div className="flex flex-row gap-1 py-1">
              {test_result_url.text} <ExternalLink className="w-4 h-4" />
            </div>
          </VSCodeLink>
        </div>
      )}
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
        <VSCodePanelTab id={`test-logs`}>
          <div className="flex flex-row gap-1">
            {test_result_exit_status === undefined && <VSCodeProgressRing className="h-4" />}
            {test_result_exit_status === 'ERROR' && <AlertTriangle className="w-4 h-4" />} Output
          </div>
        </VSCodePanelTab>
        <VSCodePanelView id={`view-logs`}>
          <ScrollArea type="always" className="flex w-full h-full pr-3">
            <TestLogPanel />
          </ScrollArea>
        </VSCodePanelView>
      </VSCodePanels>
    </>
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
