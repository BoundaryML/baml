import { Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useSelections } from './hooks'
import { DataTable } from './TestResults/data-table'
import { columns } from './TestResults/columns'
import {
  VSCodeButton,
  VSCodeLink,
  VSCodePanelTab,
  VSCodePanelView,
  VSCodePanels,
  VSCodeProgressRing,
} from '@vscode/webview-ui-toolkit/react'
import { useMemo, useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Button } from '@/components/ui/button'
import Link from './Link'
import { AlertTriangle, Download, ExternalLink, FileWarningIcon } from 'lucide-react'
import AnsiText from '@/utils/AnsiText'
import Papa from 'papaparse'
import { vscode } from '@/utils/vscode'

const TestResultPanel = () => {
  const { test_results, test_result_url, test_result_exit_status } = useSelections()

  const [selected, setSelection] = useState<string>('summary')

  if (!test_results) {
    return (
      <>
        <div className="flex flex-col items-center justify-center">
          <div className="flex flex-col items-center justify-center space-y-2">
            <div className="text-base font-semibold text-vscode-descriptionForeground">
              No test results for this function
            </div>
            <div className="text-sm font-light">Run tests to see results</div>
          </div>
        </div>
      </>
    )
  }

  return (
    <>
      {test_result_url && (
        <div className="flex flex-row items-center justify-center w-full bg-vscode-menu-background">
          <VSCodeLink href={test_result_url.url}>
            <div className="flex flex-row gap-1 py-0.5 text-xs">
              {test_result_url.text} <ExternalLink className="w-4 h-4" />
            </div>
          </VSCodeLink>
        </div>
      )}
      <div className="relative flex flex-col w-full h-full">
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
          <VSCodePanelView id={`view-summary`} className="">
            <div className="flex flex-col w-full gap-y-1">
              {test_result_exit_status === 'ERROR' && (
                <div className="flex flex-row items-center justify-center w-full h-full space-x-2">
                  <div className="flex flex-col items-center justify-center space-y-2">
                    <div className="flex flex-row items-center gap-x-2">
                      <AlertTriangle className="w-4 h-4 text-vscode-editorWarning-foreground" />
                      <div className="text-xs text-vscode-editorWarning-foreground">Test exited with an error</div>
                    </div>

                    <div className="text-xs font-light">Check the output tab for more details</div>
                  </div>
                </div>
              )}
              <DataTable columns={columns} data={test_results} />
            </div>
          </VSCodePanelView>
          <VSCodePanelTab id={`test-logs`}>
            <div className="flex flex-row gap-1">
              {test_result_exit_status === 'RUNNING' && <VSCodeProgressRing className="h-4" />}
              {test_result_exit_status === 'ERROR' && (
                <AlertTriangle className="w-4 h-4 text-vscode-editorWarning-foreground" />
              )}{' '}
              Output
            </div>
          </VSCodePanelTab>
          <VSCodePanelView id={`view-logs`}>
            <ScrollArea type="always" className="flex w-full h-full pr-3">
              <TestLogPanel />
            </ScrollArea>
          </VSCodePanelView>
        </VSCodePanels>
        {test_result_exit_status === 'COMPLETED' || test_result_exit_status === 'ERROR' ? (
          <div className="absolute right-0 z-20 top-1">
            <Button
              className="flex flex-row px-2 py-1 rounded-sm bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground w-fit h-fit whitespace-nowrap gap-x-1"
              onClick={() => {
                const test_csv = test_results.map((test) => ({
                  function_name: test.functionName,
                  test_name: test.testName,
                  impl_name: test.implName,
                  input: test.input,
                  output_raw: test.output.raw,
                  output_parsed: test.output.parsed,
                  output_error: test.output.error,
                  status: test.status,
                  url: test.url,
                }))
                vscode.postMessage({
                  command: 'downloadTestResults',
                  data: Papa.unparse(test_csv),
                })
              }}
            >
              <Download className="w-4 h-4" />
              <span className="pl-1 text-xs">CSV</span>
            </Button>
          </div>
        ) : null}
      </div>
    </>
  )
}

const TestLogPanel = () => {
  const { test_log } = useSelections()

  return (
    <div className="h-full overflow-auto text-xs bg-vscode-terminal-background">
      {test_log ? (
        <AnsiText text={test_log} className="w-full break-all whitespace-break-spaces bg-inherit text-inherit" />
      ) : (
        <div className="flex flex-col items-center justify-center w-full h-full space-y-2">Waiting</div>
      )}
    </div>
  )
}

export default TestResultPanel
