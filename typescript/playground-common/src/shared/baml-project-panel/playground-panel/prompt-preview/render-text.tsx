import { atom, useAtomValue } from 'jotai'
import { renderModeAtom } from '../preview-toolbar'
import { useMemo, useState } from 'react'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { ScrollArea } from '@/components/ui/scroll-area'
import { TokenEncoderCache } from './render-tokens'
export const isDebugModeAtom = atom((get) => get(renderModeAtom) === 'tokens')

export const RenderPromptPart: React.FC<{
  text: string
  highlightChunks?: string[]
  model?: string
  provider?: string
}> = ({ text, highlightChunks = [], model, provider }) => {
  const isDebugMode = useAtomValue(isDebugModeAtom)
  const isLongText = useMemo(() => text.split('\n').length > 18, [text])
  // const currentClient = useAtomValue(currentClientsAtom)
  // this causes weird scroll issues
  const [isFullTextVisible, setIsFullTextVisible] = useState(true)

  const tokenizer = useMemo(() => {
    if (!isDebugMode) return undefined

    // TODO! Change this to the appropriate tokenizer!
    const encodingName = TokenEncoderCache.getEncodingNameForModel('baml-openai-chat', 'gpt-4o')
    console.log('encoding name', encodingName)
    if (!encodingName) return undefined

    const enc = TokenEncoderCache.INSTANCE.getEncoder(encodingName)
    return { enc, tokens: enc.encode(text) }
  }, [text, isDebugMode, model, provider])

  // Only compute highlighted text if we're not tokenizing
  const renderContent = useMemo(() => {
    if (tokenizer) {
      const tokenized = Array.from(tokenizer.tokens).map((token) => tokenizer.enc.decode([token]))
      let result = ''
      tokenized.forEach((token, i) => {
        // don't uncomment this next line, it helps tailwind actually apply the classes
        // bg-fuchsia-800 bg-emerald-700 bg-yellow-600 bg-red-700 bg-cyan-700
        const bgClass = `bg-${['fuchsia-800', 'emerald-700', 'yellow-600', 'red-700', 'cyan-700'][i % 5]}`
        result += `<span class="${bgClass} text-white">${token}</span>`
      })
      return result
    }

    // Only do highlighting if we're not tokenizing
    if (!highlightChunks?.length) return text

    try {
      let result = text
      highlightChunks.forEach((chunk) => {
        if (!chunk) return
        if (chunk.length > 30000) return
        const regex = new RegExp(`(${chunk.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'g')
        result = result.replace(
          regex,
          '<mark class="bg-yellow-100/70 text-yellow-900 dark:bg-yellow-800/30 dark:text-yellow-100 rounded-sm px-0.5">$1</mark>',
        )
      })
      return result
    } catch (e) {
      console.error('Error highlighting text', e)
      return text
    }
  }, [text, highlightChunks, tokenizer])

  return (
    <div className='flex flex-col'>
      {isDebugMode && (
        <div className='flex flex-row gap-4 justify-start items-center px-3 py-2 text-xs border-b border-border bg-muted text-muted-foreground'>
          <div className='flex items-center gap-1.5'>
            <span className='text-muted-foreground/60'>Characters:</span>
            <span className='font-medium'>{text.length}</span>
          </div>
          <div className='flex items-center gap-1.5'>
            <span className='text-muted-foreground/60'>Words:</span>
            <span className='font-medium'>{text.split(/\s+/).filter(Boolean).length}</span>
          </div>
          <div className='flex items-center gap-1.5'>
            <span className='text-muted-foreground/60'>Lines:</span>
            <span className='font-medium'>{text.split('\n').length}</span>
          </div>
          <div className='flex items-center gap-1.5'>
            <span className='text-muted-foreground/60'>Tokens (est.):</span>
            <span className='font-medium'>{Math.ceil(text.length / 4)}</span>
          </div>
        </div>
      )}
      <ScrollArea className='relative flex-1 p-2 pb-6 bg-muted/50 dark:bg-slate-900' type='always'>
        {/* This should match the ParsedResponseRenderer max-h- as well */}
        <pre
          className={`whitespace-pre-wrap text-xs  ${isFullTextVisible ? 'max-h-[500px]' : 'max-h-64'}`}
          dangerouslySetInnerHTML={{ __html: renderContent }}
        />

        {isLongText && (
          <button
            onClick={() => setIsFullTextVisible(!isFullTextVisible)}
            className='flex absolute right-0 bottom-0 gap-1 items-center p-2 text-xs rounded-tr-md rounded-bl-md transition-colors bg-muted/50 text-muted-foreground hover:text-foreground'
          >
            {isFullTextVisible ? (
              <>
                Show less
                <ChevronUp className='w-3 h-3' />
              </>
            ) : (
              <>
                Show more
                <ChevronDown className='w-3 h-3' />
              </>
            )}
          </button>
        )}
      </ScrollArea>
    </div>
  )
}
