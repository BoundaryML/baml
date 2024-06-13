/// Content once a function has been selected.

import { TestStatus } from '@baml/common'
import type { Impl } from '@baml/common'
import {
  VSCodeBadge,
  VSCodeCheckbox,
  VSCodePanelTab,
  VSCodePanelView,
  VSCodePanels,
  VSCodeProgressRing,
} from '@vscode/webview-ui-toolkit/react'
import clsx from 'clsx'
import {
  type Tiktoken,
  type TiktokenEncoding,
  type TiktokenModel,
  getEncoding,
  getEncodingNameForModel,
} from 'js-tiktoken'
import { useMemo, useState } from 'react'
import { Checkbox } from '../components/ui/checkbox'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../components/ui/tooltip'

const Whitespace: React.FC<{ char: 'space' | 'tab' }> = ({ char }) => (
  <span className='opacity-50 text-vscode-descriptionForeground'>{char === 'space' ? <>&middot;</> : <>&rarr;</>}</span>
)

const InvisibleUtf: React.FC<{ text: string }> = ({ text }) => (
  <span className='text-xs text-red-500 opacity-75'>
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

const TOKEN_BG_STYLES = ['bg-fuchsia-800', 'bg-emerald-700', 'bg-yellow-600', 'bg-red-700', 'bg-cyan-700']

// Function to replace whitespace characters with visible characters
const replaceWhitespace = (char: string, key: string) => {
  if (char === ' ') return <Whitespace key={key} char='space' />
  if (char === '\t') return <Whitespace key={key} char='tab' />
  return char
}

const renderLine = ({
  text,
  showWhitespace,
  wrapText,
}: {
  text: string
  showWhitespace: boolean
  wrapText: boolean
}) => {
  // Split the text into segments
  const segments = text.split(whitespaceRegexp)

  // Map segments to appropriate components or strings
  const formattedText = segments.map((segment, index) => {
    if (showWhitespace && new RegExp(`^[${VISIBLE_WHITESPACE}]+$`).test(segment)) {
      return segment.split('').map((char, charIndex) => replaceWhitespace(char, index.toString() + charIndex))
    } else if (new RegExp(`^[${INVISIBLE_CODES}]+$`).test(segment)) {
      if (segment == '/') {
        return segment
      }
      return <InvisibleUtf key={index} text={segment} />
    } else {
      return segment
    }
  })
  return showWhitespace ? (
    <div className={clsx('inline-flex text-xs', { 'flex-wrap': wrapText })}>{formattedText}</div>
  ) : (
    <div className='whitespace-pre-wrap'>{formattedText}</div>
  )
}

const CodeLine: React.FC<{
  // line is either the entire line, or when tokenization is on, an array of [token, tokenIndex]
  line: string | [string, number][]
  lineNumber: number
  showWhitespace: boolean
  wrapText: boolean
  maxLineNumber: number
}> = ({ line, lineNumber, showWhitespace, wrapText, maxLineNumber }) => {
  // Function to render whitespace characters and invisible UTF characters with special styling
  const lineNumberSpan = (
    <span className='pr-1 font-mono text-xs text-right text-gray-500 select-none'>
      {lineNumber.toString().padStart(maxLineNumber.toString().length, ' ')}
    </span>
  )

  const isTokenized = Array.isArray(line[0])

  if (Array.isArray(line)) {
    console.log('line', line)
    return (
      <div className='flex flex-row items-start'>
        {lineNumberSpan}
        <div className='text-wrap'>
          {line.map(([token, tokenIndex], index) => (
            <span
              className={clsx('inline-flex font-mono text-xs', TOKEN_BG_STYLES[tokenIndex % TOKEN_BG_STYLES.length], {
                'whitespace-pre-wrap': wrapText || isTokenized,
                'text-white': isTokenized,
                "after:content-['↵']": index === line.length - 1,
                'after:opacity-50': index === line.length - 1,
              })}
            >
              {renderLine({ text: token, showWhitespace, wrapText })}
            </span>
          ))}
        </div>
      </div>
    )
  }

  return (
    <div className='flex flex-row items-start'>
      {lineNumberSpan}
      <span className={clsx('inline-block font-mono text-xs', { 'whitespace-pre-wrap': wrapText })}>
        {renderLine({ text: line, showWhitespace, wrapText })}
      </span>
    </div>
  )
}

// We need this cache because loading a new encoder for every snippet makes rendering horribly slow
class TokenEncoderCache {
  static SUPPORTED_PROVIDERS = [
    'baml-openai-chat',
    'baml-openai-completion',
    'baml-azure-chat',
    'baml-azure-completion',
  ]
  static INSTANCE = new TokenEncoderCache()

  encoders: Map<TiktokenEncoding, Tiktoken>

  private constructor() {
    this.encoders = new Map()
  }

  static getEncodingNameForModel(provider: string, model: string): TiktokenEncoding | undefined {
    if (!TokenEncoderCache.SUPPORTED_PROVIDERS.includes(provider)) return undefined

    // We have to use this try-catch approach because tiktoken does not expose a list of supported models
    try {
      return getEncodingNameForModel(model as TiktokenModel)
    } catch {
      return undefined
    }
  }

  getEncoder(encoding: TiktokenEncoding): Tiktoken {
    const cached = this.encoders.get(encoding)
    if (cached) return cached

    const encoder = getEncoding(encoding)
    this.encoders.set(encoding, encoder)
    return encoder
  }
}

export const Snippet: React.FC<{
  text: string
  type?: 'preview' | 'error'
  client: Impl['client']
}> = ({ text, type = 'preview', client }) => {
  const [showTokens, setShowTokens] = useState(false)
  const [showWhitespace, setShowWhitespace] = useState(false)
  const [wrapText, setWrapText] = useState(true)

  const encodingName = client.model
    ? TokenEncoderCache.getEncodingNameForModel(client.provider, client.model)
    : undefined

  const tokenizer = useMemo(() => {
    if (!showTokens) return undefined
    if (!encodingName) return undefined

    const enc = TokenEncoderCache.INSTANCE.getEncoder(encodingName)
    return { enc, tokens: enc.encode(text) }
  }, [text, encodingName, showTokens])

  const divStyle = clsx('r-full', 'p-1', 'overflow-hidden', 'rounded-lg', {
    'bg-vscode-input-background': type === 'preview',
    'bg-vscode-inputValidation-errorBackground': type === 'error',
  })

  const header = (
    <div className='flex flex-wrap justify-start gap-4 px-2 py-2 text-xs whitespace-nowrap'>
      {encodingName && (
        <PromptCheckbox checked={showTokens} onChange={(e) => setShowTokens(e)}>
          Show Tokens
        </PromptCheckbox>
      )}
      <PromptCheckbox checked={wrapText} onChange={(e) => setWrapText(e)}>
        Wrap Text
      </PromptCheckbox>
      <PromptCheckbox checked={showWhitespace} onChange={(e) => setShowWhitespace(e)}>
        Whitespace
      </PromptCheckbox>

      {showTokens && encodingName && tokenizer && (
        <Tooltip delayDuration={0}>
          <TooltipTrigger asChild>
            <div className='flex-grow p-0 r-full ps-2'>{(tokenizer.tokens as []).length} tokens</div>
          </TooltipTrigger>
          <TooltipContent className='flex flex-col gap-y-1'>
            Tokenizer {encodingName} for model {client.model}
          </TooltipContent>
        </Tooltip>
      )}
    </div>
  )

  if (showTokens && tokenizer) {
    const tokenized = Array.from(tokenizer.tokens as number[]).map((token) => tokenizer.enc.decode([token]))
    const tokenizedLines: [string, number][][] = [[]]
    tokenized.forEach((token, tokenIndex) => {
      const noNewlines = token.split('\n')
      ;(tokenizedLines.at(-1) as [string, number][]).push([noNewlines.at(0) as string, tokenIndex])
      for (let i = 1; i < noNewlines.length; i++) {
        tokenizedLines.push([['', tokenIndex]])
      }
    })
    const tokenizedContent = tokenizedLines.map((line, lineIndex) => (
      <CodeLine
        key={lineIndex}
        line={line}
        lineNumber={lineIndex + 1}
        maxLineNumber={tokenizedLines.length}
        showWhitespace={false}
        wrapText={false}
      />
    ))
    return (
      <div className={divStyle}>
        {header}
        <pre className='w-full p-1 text-xs'>{tokenizedContent}</pre>
      </div>
    )
  } else {
    const lines = text.split('\n')
    return (
      <div className={divStyle}>
        {header}
        <pre className='w-full p-1 text-xs'>
          {lines.map((line, index) => (
            <CodeLine
              key={index}
              maxLineNumber={lines.length}
              line={line}
              lineNumber={index + 1}
              showWhitespace={showWhitespace}
              wrapText={wrapText}
            />
          ))}
        </pre>
      </div>
    )
  }
}

const PromptCheckbox = ({
  children,
  checked,
  onChange,
}: {
  children: React.ReactNode
  checked: boolean
  onChange: (e: any) => void
}) => {
  return (
    <div className='flex flex-row items-center gap-1'>
      <Checkbox checked={checked} onCheckedChange={onChange} className='border-vscode-descriptionForeground' />
      <span className='text-vscode-descriptionForeground'>{children}</span>
    </div>
  )
}

const PromptPreview: React.FC<{ prompt: Impl['prompt']; client: Impl['client']; shouldSuppressError: boolean }> = ({
  prompt,
  client,
  shouldSuppressError,
}) => {
  switch (prompt.type) {
    case 'Completion':
      return <Snippet client={client} text={prompt.completion} />
    case 'Chat':
      return (
        <div className='flex flex-col gap-2'>
          {prompt.chat.map(({ role, message }, index: number) => (
            <div className='flex flex-col'>
              <div className='text-xs'>
                <span className='text-muted-foreground'>Role:</span> <span className='font-bold'>{role}</span>
              </div>
              <Snippet key={index} client={client} text={message} />
            </div>
          ))}
        </div>
      )
    case 'Error':
      if (shouldSuppressError) {
        return null
      }
      return <Snippet type='error' client={client} text={prompt.error} />
  }
}
