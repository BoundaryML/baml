import { PropsWithChildren } from "react"
import { TestState, TestStatusType } from "./testHooks"
import { VSCodeProgressRing } from "@vscode/webview-ui-toolkit/react"

const TestStatusIcon: React.FC<PropsWithChildren<{ testStatus: TestStatusType }>> = ({ testStatus, children }) => {
  return (
    <div className="text-vscode-descriptionForeground">
      {
        {
          ['queued']: 'Queued',
          ['running']: <VSCodeProgressRing className="h-4" />,
          ['done']: (
            <div className="flex flex-row items-center gap-1">
              <div className="text-vscode-testing-iconPassed">Passed</div>
              {children}
            </div>
          ),
          ['error']: (
            <div className="flex flex-row items-center gap-1">
              <div className="text-vscode-testing-iconFailed">Failed</div>
              {children}
            </div>
          ),
        }[testStatus]
      }
    </div>
  )
}

const TestRow: React.FC<{ test: TestState }> = ({ test }) => {
  return (
    <div className="flex flex-row items-center gap-2">
      <TestStatusIcon testStatus={test.status}>
        {test.status == 'done' && test.response}
        {test.status == 'error' && test.message}
      </TestStatusIcon>
    </div>
  )
}