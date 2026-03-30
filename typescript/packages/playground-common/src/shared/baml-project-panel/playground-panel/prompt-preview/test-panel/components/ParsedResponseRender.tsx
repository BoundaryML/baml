import JsonView from '@uiw/react-json-view'
import { githubLightTheme as lightTheme } from '@uiw/react-json-view/githubLight'
import { vscodeTheme as darkTheme } from '@uiw/react-json-view/vscode'
import type { TestResponseData } from '../../../../../../sdk/interface'
import { useTheme } from 'next-themes'
import { RenderPromptPart } from '../../render-text'
import { ScrollArea } from '@baml/ui/scroll-area'
import { useState } from 'react'
import { AlertTriangle, ChevronDown, ChevronUp } from 'lucide-react'

const ErrorText = ({ text }: { text: string }) => {
  return <pre className='text-xs text-red-500 whitespace-pre-wrap'>{text}</pre>
}

const ExplanationView = ({ explanation }: { explanation: string }) => {
  const [isExpanded, setIsExpanded] = useState(false)

  let parsed: unknown[];
  try {
    parsed = JSON.parse(explanation)
    if (!Array.isArray(parsed) || parsed.length === 0) return null
  } catch {
    return null
  }

  return (
    <div className='mt-2 rounded-md border border-yellow-500/30 bg-yellow-500/5 p-2'>
      <button
        className='flex items-center gap-1 text-xs text-yellow-600 dark:text-yellow-400 hover:underline'
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <AlertTriangle className='w-3 h-3' />
        <span>
          {parsed.length} parsing {parsed.length === 1 ? 'issue' : 'issues'}{' '}
          (items may have been dropped)
        </span>
        {isExpanded ? (
          <ChevronUp className='w-3 h-3' />
        ) : (
          <ChevronDown className='w-3 h-3' />
        )}
      </button>
      {isExpanded && (
        <pre className='mt-2 text-xs text-yellow-700 dark:text-yellow-300 whitespace-pre-wrap max-h-[300px] overflow-auto'>
          {JSON.stringify(parsed, null, 2)}
        </pre>
      )}
    </div>
  )
}

// Renders the parsed response only
export const ParsedResponseRenderer: React.FC<{
  response?: TestResponseData
}> = ({ response }) => {
  if (!response) {
    return <div className='text-xs text-muted-foreground'>Waiting for response...</div>
  }

  const llmFailure = response.llm_failure
  const failureMessage = response.failure_message

  if (llmFailure) {
    return <ErrorText text={llmFailure.message} />
  }

  const parsedResponse = response.parsed_response

  return (
    <div>
      <ParsedResponseRender response={parsedResponse?.value} />
      {failureMessage && <ErrorText text={failureMessage} />}
      {parsedResponse?.explanation && (
        <ExplanationView explanation={parsedResponse.explanation} />
      )}
    </div>
  )
}

const ParsedResponseRender = ({ response }: { response: string | undefined }) => {
  const { theme } = useTheme()

  if (!response) {
    return null
  }

  let parsedResponseObj
  try {
    parsedResponseObj = JSON.parse(response)
    if (parsedResponseObj === null || typeof parsedResponseObj !== 'object') {
      return <RenderPromptPart text={response} />
    }
  } catch (e) {
    parsedResponseObj = response
  }

  if (typeof parsedResponseObj === 'string') {
    return <RenderPromptPart text={parsedResponseObj} />
  }

  return (
    <div className='flex max-h-[600px]  text-xs'>
      <ScrollArea className='pr-2 w-full text-xs' type='always'>
        <JsonView
          className='p-1 w-full rounded-md'
          value={parsedResponseObj || {}}
          collapsed={false}
          enableClipboard={true}
          displayDataTypes={false}
          displayObjectSize={true}
          indentWidth={16}
          shortenTextAfterLength={700}
          style={theme === 'dark' ? darkTheme : lightTheme}
        >
          <JsonView.String
            render={({ children, style: _style, ...reset }, { type, value, keyName }) => {
              if (type === 'type') {
                return <span />
              }
              if (type === 'value') {
                return (
                  <span {...reset} className='whitespace-pre-wrap break-all'>
                    &quot;{children}&quot;<span className='text-muted-foreground'>, </span>
                  </span>
                )
              }
            }}
          />
          <JsonView.Colon
            render={({ style: _style, ...props }, { parentValue, value, keyName }) => {
              if (Array.isArray(parentValue) && props.children == ':') {
                return <span />
              }
              return <span {...props}>: </span>
            }}
          />

          <JsonView.Null
            render={({ children, style: _style, ...reset }) => (
              <span {...reset} className='whitespace-pre-wrap break-words'>
                null
              </span>
            )}
          />
          <JsonView.Undefined
            render={({ children, style: _style, ...reset }) => (
              <span {...reset} className='whitespace-pre-wrap break-words'>
                undefined
              </span>
            )}
          />
          <JsonView.KeyName
            render={(
              { style: _style, ...props },
              { parentValue, value, keyName },
            ) => {
              if (
                Array.isArray(parentValue) &&
                Number.isFinite(props.children)
              ) {
                return <span className='' />;
              }
              return <span className='' {...props} />
            }}
          />
        </JsonView>
      </ScrollArea>
    </div>
  )
}
