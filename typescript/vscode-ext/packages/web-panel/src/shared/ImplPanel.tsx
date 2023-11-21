/// Content once a function has been selected.

import { ParserDatabase, TestResult, TestStatus } from '@baml/common'
import { useImplCtx } from './hooks'
import {
  VSCodeBadge,
  VSCodeCheckbox,
  VSCodeLink,
  VSCodePanelTab,
  VSCodePanelView,
  VSCodeProgressRing,
} from '@vscode/webview-ui-toolkit/react'
import { vscode } from '@/utils/vscode'
import { useMemo, useState } from 'react'
import Link from './Link'
import TypeComponent from './TypeComponent'
import { ArgType } from '@baml/common/src/parser_db'

type Impl = ParserDatabase['functions'][0]['impls'][0]

const TestStatusIcon = ({ testStatus }: { testStatus: TestStatus }) => {
  return (
    <div className="text-vscode-descriptionForeground">
      {
        {
          [TestStatus.Queued]: 'Queued',
          [TestStatus.Running]: <VSCodeProgressRing className="h-4" />,
          [TestStatus.Passed]: <div className="text-vscode-testing-iconPassed">Passed</div>,
          [TestStatus.Failed]: <div className="text-vscode-testing-iconFailed">Failed</div>,
        }[testStatus]
      }
    </div>
  )
}

const ImplPanel: React.FC<{ impl: Impl }> = ({ impl }) => {
  const { func, test_result } = useImplCtx(impl.name.value)
  const [showPrompt, setShowPrompt] = useState(true)

  const implPrompt = useMemo(() => {
    let prompt = impl.prompt
    impl.input_replacers.forEach(({ key, value }) => {
      prompt = prompt.replaceAll(key, `{${value}}`)
    })
    impl.output_replacers.forEach(({ key, value }) => {
      prompt = prompt.replaceAll(key, value)
    })
    return prompt
  }, [impl.prompt, impl.input_replacers, impl.output_replacers])

  if (!func) return null

  return (
    <>
      <VSCodePanelTab key={`tab-${impl.name.value}`} id={`tab-${func.name.value}-${impl.name.value}`}>
        <div className="flex flex-row gap-1">
          <span>{impl.name.value}</span>
          {test_result && (
            <VSCodeBadge>
              <TestStatusIcon testStatus={test_result.status} />
            </VSCodeBadge>
          )}
        </div>
      </VSCodePanelTab>
      <VSCodePanelView key={`view-${impl.name.value}`} id={`view-${func.name.value}-${impl.name.value}`}>
        <div className="flex flex-col w-full gap-2">
          {test_result && <TestResultPanel testResult={test_result} output={func.output} />}
          <div className="flex flex-row gap-1">
            <span className="font-bold">Client</span>
            <Link item={impl.client} />
          </div>

          <div className="flex flex-col gap-1">
            <div className="flex flex-row justify-between items-center">
              <span className="flex gap-1">
                <b>Prompt</b>
                <Link item={impl.name} display="Edit" />
              </span>
              <div className="flex flex-row gap-1 items-center">
                <VSCodeCheckbox
                  checked={showPrompt}
                  onChange={(e) => setShowPrompt((e as React.FormEvent<HTMLInputElement>).currentTarget.checked)}
                >
                  Show Prompt
                </VSCodeCheckbox>
              </div>
            </div>
            {showPrompt && (
              <pre className="w-full p-2 overflow-y-scroll whitespace-pre-wrap bg-vscode-input-background">
                {implPrompt}
              </pre>
            )}
          </div>
        </div>
      </VSCodePanelView>
    </>
  )
}

const TestResultPanel: React.FC<{ output: ArgType; testResult: TestResult }> = ({ output, testResult }) => {
  const output_string = useMemo((): string | null => {
    if (!testResult.output) return null

    if (typeof testResult.output === 'string') {
      try {
        let parsed = JSON.parse(testResult.output)
        if (typeof parsed === 'object') return JSON.stringify(parsed, null, 2)
        return parsed
      } catch (e) {
        return testResult.output
      }
    } else {
      return JSON.stringify(testResult.output, null, 2)
    }
  }, [testResult.output])

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-row justify-between items-center">
        <span className="flex gap-1">
          <b>output</b> {output.arg_type === 'positional' && <TypeComponent typeString={output.type} />}
        </span>
        {testResult.status && <TestStatusIcon testStatus={testResult.status} />}
      </div>
      <div className="max-w-full">
        <pre className="w-full h-full min-h-[80px] p-1 overflow-y-scroll break-words whitespace-break-spaces bg-vscode-input-background">
          {output_string ?? (
            <div className="flex flex-col items-center justify-center h-full text-vscode-descriptionForeground">
              <div>Nothing here yet...</div>
            </div>
          )}
        </pre>
      </div>
    </div>
  )
}

export default ImplPanel
