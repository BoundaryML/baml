// ResponseRenderer.tsx
import { WasmFunctionResponse, WasmTestResponse, WasmLLMFailure, WasmLLMResponse } from '@gloo-ai/baml-schema-wasm-web'
import { DoneTestStatusType } from '../../../atoms'
import { useState } from 'react'
import {
  AlertCircle,
  Brain,
  Check,
  CheckCircle,
  ChevronDown,
  ChevronsLeftRight,
  ChevronUp,
  Clock,
  Copy,
} from 'lucide-react'
import { Button } from '~/components/ui/button'
import { Badge } from '~/components/ui/badge'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '~/components/ui/tooltip'
import { ParsedResponseRenderer } from './ParsedResponseRender'
import { RenderPromptPart } from '../../render-text'

interface ResponseRendererProps {
  response?: WasmFunctionResponse | WasmTestResponse
  status?: DoneTestStatusType
}

// Renders both the raw LLM response and the parsed response
export const ResponseRenderer: React.FC<ResponseRendererProps> = ({ response, status }) => {
  const [parsedCopied, setParsedCopied] = useState(false)
  const [llmCopied, setLlmCopied] = useState(false)

  if (!response) {
    return <div className='text-xs text-muted-foreground'>Waiting for response...</div>
  }

  const llmFailure = response.llm_failure()
  const llmResponse = response.llm_response()
  const parsedResponse = response.parsed_response()
  const failureMessage = 'failure_message' in response ? response.failure_message() : undefined

  if (llmFailure) {
    return <LLMFailureView failure={llmFailure} />
  }

  const handleLlmCopy = () => {
    navigator.clipboard.writeText(llmResponse?.content ?? '')
    setLlmCopied(true)
    setTimeout(() => setLlmCopied(false), 2000)
  }

  const handleParsedCopy = () => {
    if (!parsedResponse) return
    const value = typeof parsedResponse === 'string' ? parsedResponse : parsedResponse.value
    navigator.clipboard.writeText(JSON.stringify(JSON.parse(value ?? ''), null, 2))
    setParsedCopied(true)
    setTimeout(() => setParsedCopied(false), 2000)
  }

  // Helper to determine if we should show parsed response separately
  const shouldShowParsedSeparately = () => {
    // if (!parsedResponse || !llmResponse) return false
    // const parsedValue = typeof parsedResponse === 'string' ? parsedResponse : parsedResponse.value
    // return parsedValue && JSON.stringify(JSON.parse(parsedValue)) !== JSON.stringify(llmResponse.content)
    return true
  }

  return (
    <div className='space-y-4'>
      {/* Metadata Section */}
      {llmResponse && (
        <div className='flex flex-wrap gap-2'>
          <MetadataBadges llmResponse={llmResponse} />
        </div>
      )}

      {/* Content Section */}
      <div className={`grid ${shouldShowParsedSeparately() ? 'grid-cols-2' : 'grid-cols-1'} gap-4`}>
        {/* LLM Response */}
        {llmResponse && (
          <div className='relative group'>
            <div className='space-y-2'>
              <span className='text-xs text-muted-foreground'>Raw LLM Response</span>
              <RenderPromptPart text={llmResponse.content} />
            </div>
            <CopyButton copied={llmCopied} onCopy={handleLlmCopy} />
          </div>
        )}

        {/* Parsed Response */}
        {shouldShowParsedSeparately() && (
          <div className='relative group'>
            <div className='space-y-2'>
              <span className='flex flex-row gap-x-1 text-xs text-muted-foreground'>
                <div>Parsed Response</div>
                {parsedResponse && typeof parsedResponse !== 'string' && parsedResponse.check_count > 0 ? (
                  <div className='flex items-center space-x-1'>
                    {/* <CheckCircle className="w-3 h-3" /> */}
                    <span>({parsedResponse.check_count} checks ran)</span>
                  </div>
                ) : null}
              </span>
              <ParsedResponseRenderer response={response} />
            </div>
            <CopyButton copied={parsedCopied} onCopy={handleParsedCopy} />
          </div>
        )}
      </div>

      {/* Error Messages are rendered inside ParsedResponseRenderer*/}
    </div>
  )
}

// Renders the raw response only
export const RawResponseRenderer: React.FC<{
  response?: WasmFunctionResponse | WasmTestResponse
}> = ({ response }) => {
  if (!response) {
    return <div className='text-xs text-muted-foreground'>Waiting for response...</div>
  }
  return <RenderPromptPart text={response.llm_response()?.content ?? ''} />
}

const MetadataBadges: React.FC<{ llmResponse: WasmLLMResponse }> = ({ llmResponse }) => (
  <TooltipProvider>
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant='outline' className='flex items-center space-x-1 font-light text-muted-foreground'>
          <Brain className='w-3 h-3' />
          <span>{llmResponse.model}</span>
        </Badge>
      </TooltipTrigger>
      <TooltipContent>Model</TooltipContent>
    </Tooltip>

    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant='outline' className='flex items-center space-x-1 font-light text-muted-foreground'>
          <Clock className='w-3 h-3' />
          <span>{(Number(llmResponse.latency_ms) / 1000).toFixed(2)}s</span>
        </Badge>
      </TooltipTrigger>
      <TooltipContent>Latency</TooltipContent>
    </Tooltip>
    <Tooltip delayDuration={0}>
      <TooltipTrigger asChild>
        <Badge variant='outline' className='flex items-center space-x-1 font-light text-muted-foreground'>
          <ChevronsLeftRight className='w-3 h-3' />
          <span>
            {llmResponse.input_tokens != null ? Number(llmResponse.input_tokens) : 'unknown'} in |{' '}
            {llmResponse.output_tokens != null ? Number(llmResponse.output_tokens) : 'unknown'} out
          </span>
        </Badge>
      </TooltipTrigger>
      <TooltipContent>Tokens (in / out)</TooltipContent>
    </Tooltip>
  </TooltipProvider>
)

const CopyButton: React.FC<{ copied: boolean; onCopy: () => void }> = ({ copied, onCopy }) => (
  <Button
    variant='ghost'
    size='icon'
    className='absolute top-0 right-0 w-4 h-4 opacity-0 transition-opacity bg-muted group-hover:opacity-100'
    onClick={onCopy}
  >
    {copied ? <Check className='w-4 h-4' /> : <Copy className='w-4 h-4' />}
  </Button>
)

const LLMFailureView: React.FC<{ failure: WasmLLMFailure }> = ({ failure }) => {
  const [isExpanded, setIsExpanded] = useState(false)

  return (
    <div className='space-y-3 text-xs'>
      <div className='flex items-center space-x-2 text-destructive'>
        <AlertCircle className='w-4 h-4' />
        <span className='font-semibold'>{failure.code}</span>
      </div>

      <Button variant='ghost' size='sm' onClick={() => setIsExpanded(!isExpanded)} className='p-0 h-auto font-normal'>
        {isExpanded ? (
          <>
            <ChevronUp className='mr-1 w-4 h-4' />
            Hide full message
          </>
        ) : (
          <>
            <ChevronDown className='mr-1 w-4 h-4' />
            Show full message
          </>
        )}
      </Button>

      {isExpanded && (
        <div className='p-3 mt-2 font-mono text-xs whitespace-pre-wrap rounded-md bg-muted'>{failure.message}</div>
      )}

      {/* <MetadataBadges llmResponse={failure.} /> */}
    </div>
  )
}
