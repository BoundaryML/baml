/// Content once a function has been selected.

import { ParserDatabase, TestResult, TestStatus } from '@baml/common'
import { useImplCtx, useSelections } from './hooks'
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
import { Impl } from '@baml/common/src/parser_db'
import { Table, TableHead } from '@/components/ui/table'
import clsx from 'clsx'

const Whitespace: React.FC<{ char: 'space' | 'tab' }> = ({ char }) => (
  <span className="opacity-50 text-vscode-descriptionForeground">{char === 'space' ? <>&middot;</> : <>&rarr;</>}</span>
)

const InvisibleUtf: React.FC<{ text: string }> = ({ text }) => (
  <span className="text-xs text-red-500 opacity-75">
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

const CodeLine: React.FC<{ line: string; number: number; showWhitespace: boolean; wrapText: boolean }> = ({
  line,
  number,
  showWhitespace,
  wrapText,
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
    return showWhitespace ? (
      <div
        className={clsx('flex text-xs', {
          'flex-wrap': wrapText,
        })}
      >
        {formattedText}
      </div>
    ) : (
      <>{formattedText}</>
    )
  }

  return (
    <div className="table-row">
      <span className="table-cell pr-2 font-mono text-xs text-right text-gray-500 select-none">{number}</span>
      <span
        className={clsx('table-cell font-mono text-xs', {
          'whitespace-pre-wrap': wrapText,
        })}
      >
        {renderLine(line)}
      </span>
    </div>
  )
}

const Snippet: React.FC<{ text: string }> = ({ text }) => {
  const [showWhitespace, setShowWhitespace] = useState(true)
  const [wrapText, setWrapText] = useState(true)

  const lines = text.split('\n')
  return (
    <div className="w-full p-1 overflow-hidden rounded-lg bg-vscode-input-background">
      <div className="flex flex-row justify-end gap-2 text-xs">
        <VSCodeCheckbox
          checked={wrapText}
          onChange={(e) => setWrapText((e as React.FormEvent<HTMLInputElement>).currentTarget.checked)}
        >
          Wrap Text
        </VSCodeCheckbox>
        <VSCodeCheckbox
          checked={showWhitespace}
          onChange={(e) => setShowWhitespace((e as React.FormEvent<HTMLInputElement>).currentTarget.checked)}
        >
          Whitespace
        </VSCodeCheckbox>
      </div>
      <pre className="w-full p-1 text-xs bg-vscode-input-background text-vscode-textPreformat-foreground">
        {lines.map((line, index) => (
          <CodeLine key={index} line={line} number={index + 1} showWhitespace={showWhitespace} wrapText={wrapText} />
        ))}
      </pre>
    </div>
  )
}

const PromptPreview: React.FC<{ prompt: Impl['prompt'] }> = ({prompt}) => {
  switch (prompt.type) {
    case "Completion":
      return <Snippet text={prompt.completion} />
    case "Chat":
      return (<div className='flex flex-col gap-2'>
              {prompt.chat.map(({ role, message }, index: number) => (
                <div className='flex flex-col'>
                  <div className='text-xs'><span className='text-muted-foreground'>Role:</span> <span className='font-bold'>{role}</span></div>
                  <Snippet key={index} text={message} />
                </div>
              ))}
            </div>);
  }
}

const ImplPanel: React.FC<{ impl: Impl }> = ({ impl }) => {
  const { func } = useImplCtx(impl.name.value)

  if (!func) return null

  return (
    <>
      <VSCodePanelTab key={`tab-${impl.name.value}`} id={`tab-${func.name.value}-${impl.name.value}`}>
        <div className="flex flex-row gap-1">
          <span>{impl.name.value}</span>
        </div>
      </VSCodePanelTab>
      <VSCodePanelView key={`view-${impl.name.value}`} id={`view-${func.name.value}-${impl.name.value}`}>
        <div className="flex flex-col w-full gap-2">
          <div className="flex flex-col gap-1">
            <div className="flex flex-row items-center justify-between">
              <span className="flex gap-1">
                <b>Prompt</b>
                <Link item={impl.name} display="Edit" />
              </span>
              <div className="flex flex-row gap-1">
                {/* <span className="font-light">Client</span> */}
                <Link item={impl.client} />
              </div>
            </div>
            <PromptPreview prompt={impl.prompt}/>
          </div>
        </div>
      </VSCodePanelView>
    </>
  )
}

export default ImplPanel
