/// Content once a function has been selected.

import { ParserDatabase, TestResult, TestStatus } from '@baml/common'
import { useImplCtx } from './hooks'
import {
  VSCodeBadge,
  VSCodeCheckbox,
  VSCodePanelTab,
  VSCodePanelView,
  VSCodePanels,
  VSCodeProgressRing,
} from '@vscode/webview-ui-toolkit/react'
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

const Whitespace: React.FC<{ char: 'space' | 'tab' }> = ({ char }) => (
  <span className="text-blue-500 opacity-75">{char === 'space' ? <>&middot;</> : <>&rarr;</>}</span>
)

const InvisibleUtf: React.FC<{ text: string }> = ({ text }) => (
  <span className="text-red-500 text-xs opacity-75">
    {text
      .split('')
      .map((c) => `U+${c.charCodeAt(0).toString(16).padStart(4, '0')}`)
      .join('')}
  </span>
)

// Excludes 0x20 (space) and 0x09 (tab)
const VISIBLE_WHITESPACE = /\u0020\u0009/
const INVISIBLE_CODES =
  /\u00a0\u00ad\u034f\u061c\u070f\u115f\u1160\u1680\u17b4\u17b5\u180e\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200a\u200b\u200c\u200d\u200e\u200f\u202f\u205f\u2060\u2061\u2062\u2063\u2064\u206a\u206b\u206c\u206d\u206e\u206f\u3000\u2800\u3164\ufeff\uffa0/
const whitespaceRegexp = new RegExp(`([${VISIBLE_WHITESPACE}]+|[${INVISIBLE_CODES}]+)`, 'g')

const CodeLine: React.FC<{ line: string; number: number; showWhitespace: boolean }> = ({
  line,
  number,
  showWhitespace,
}) => {
  // Function to render whitespace characters and invisible UTF characters with special styling
  const renderLine = (text: string) => {
    // Function to replace whitespace characters with visible characters
    const replaceWhitespace = (char: string, key: string) => {
      if (char === ' ') return <Whitespace key={key} char="space" />
      if (char === '\t') return <Whitespace key={key} char="tab" />
      return char
    }

    // Split the text into segments
    const segments = text.split(whitespaceRegexp)

    // Map segments to appropriate components or strings
    const formattedText = segments.map((segment, index) => {
      if (showWhitespace && new RegExp(`^[${VISIBLE_WHITESPACE}]+$`).test(segment)) {
        return segment.split('').map((char, charIndex) => replaceWhitespace(char, index.toString() + charIndex))
      } else if (new RegExp(`^[${INVISIBLE_CODES}]+$`).test(segment)) {
        return <InvisibleUtf key={index} text={segment} />
      } else {
        return segment
      }
    })
    return showWhitespace ? <div className="flex flex-wrap">{formattedText}</div> : <>{formattedText}</>
  }

  return (
    <div className="table-row">
      <span className="table-cell text-right pr-4 font-mono text-sm text-gray-500 select-none">{number}</span>
      <span className="table-cell font-mono text-sm whitespace-pre-wrap">{renderLine(line)}</span>
    </div>
  )
}

const Snippet: React.FC<{ text: string }> = ({ text }) => {
  const [showWhitespace, setShowWhitespace] = useState(true)

  const lines = text.split('\n')
  return (
    <div className="w-full p-2 bg-vscode-input-background rounded-lg overflow-hidden">
      <div className="flex flex-row justify-end">
        <VSCodeCheckbox
          checked={showWhitespace}
          onChange={(e) => setShowWhitespace((e as React.FormEvent<HTMLInputElement>).currentTarget.checked)}
        >
          Whitespace
        </VSCodeCheckbox>
      </div>
      <pre className="w-full p-2 overflow-y-scroll whitespace-pre-wrap bg-vscode-input-background">
        <code>
          {lines.map((line, index) => (
            <CodeLine key={index} line={line} number={index + 1} showWhitespace={showWhitespace} />
          ))}
        </code>
      </pre>
    </div>
  )
}

const ImplPanel: React.FC<{ impl: Impl }> = ({ impl }) => {
  const { func, test_result } = useImplCtx(impl.name.value)

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
          <div className="flex flex-row gap-1">
            <span className="font-bold">Client</span>
            <Link item={impl.client} />
          </div>
          <TestResultPanel testResult={test_result} output={func.output} />

          <div className="flex flex-col gap-1">
            <div className="flex flex-row justify-between items-center">
              <span className="flex gap-1">
                <b>Prompt</b>
                <Link item={impl.name} display="Edit" />
              </span>
            </div>
            <Snippet text={implPrompt} />
          </div>
        </div>
      </VSCodePanelView>
    </>
  )
}

const TestResultPanel: React.FC<{ output: ArgType; testResult?: TestResult }> = ({ output, testResult }) => {
  const [tab, setTab] = useState<'tab-impl-error' | 'tab-impl-parsed' | 'tab-impl-raw' | undefined>(undefined)

  const output_string = useMemo((): string | null => {
    if (!testResult?.output.parsed) return null

    if (typeof testResult.output.parsed === 'string') {
      try {
        let parsed = JSON.parse(testResult.output.parsed)
        if (typeof parsed === 'object') return JSON.stringify(parsed, null, 2)
        return parsed
      } catch (e) {
        return testResult.output.parsed
      }
    } else {
      return JSON.stringify(testResult.output.parsed, null, 2)
    }
  }, [testResult?.output.parsed])

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-row justify-between items-center">
        <span className="flex gap-1">
          <b>output</b> {output.arg_type === 'positional' && <TypeComponent typeString={output.type} />}
        </span>
        {testResult?.status && <TestStatusIcon testStatus={testResult.status} />}
      </div>
      <div className="max-w-full">
        <VSCodePanels
          className="w-full"
          activeid={tab}
          onChange={(e) => {
            const selected: string | undefined = (e.target as any)?.activetab?.id
            if (selected && selected.startsWith('tab-impl-')) {
              setTab(selected as any)
            }
          }}
        >
          {output_string != null && (
            <>
              <VSCodePanelTab id="tab-impl-parsed">BAML Parsed</VSCodePanelTab>
              <VSCodePanelView id="view-impl-parsed">
                {output_string && (
                  <pre className="w-full p-2 overflow-y-scroll whitespace-pre-wrap bg-vscode-input-background">
                    {output_string}
                  </pre>
                )}
              </VSCodePanelView>
            </>
          )}
          {testResult?.output.raw != undefined && (
            <>
              <VSCodePanelTab id="tab-impl-raw">Raw LLM</VSCodePanelTab>
              <VSCodePanelView id="view-impl-raw">
                {testResult?.output.raw && <Snippet text={testResult.output.raw} />}
              </VSCodePanelView>
            </>
          )}
          {testResult?.output.error != undefined && (
            <>
              <VSCodePanelTab id="tab-impl-error">Error</VSCodePanelTab>
              <VSCodePanelView id="view-impl-error">
                {testResult?.output.error && <Snippet text={testResult.output.error} />}
              </VSCodePanelView>
            </>
          )}
        </VSCodePanels>
      </div>
    </div>
  )
}

export default ImplPanel
