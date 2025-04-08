import { Check, Copy, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { useState } from 'react'
import { cn } from '@/lib/utils'

export const Loader: React.FC<{ message?: string; className?: string }> = ({ message, className }) => {
  return (
    <div className={cn('flex gap-2 justify-center items-center text-gray-500', className)}>
      <Loader2 className='animate-spin' />
      {message}
    </div>
  )
}

export const ErrorMessage: React.FC<{ error: string }> = ({ error }) => {
  return (
    <pre
      className='px-2 py-1 w-full font-mono text-red-500 whitespace-pre-wrap rounded-lg'
      style={{
        wordBreak: 'normal',
        overflowWrap: 'anywhere',
      }}
    >
      {error}
    </pre>
  )
}

export const WithCopyButton: React.FC<{
  children: React.ReactNode
  text: string
}> = ({ children, text }) => {
  const [copyState, setCopyState] = useState<'copying' | 'copied' | 'idle'>('idle')

  return (
    <div className='relative group'>
      {copyState === 'idle' && (
        <Button
          onClick={() => {
            setCopyState('copying')
            void navigator.clipboard.writeText(text).then(() => {
              setCopyState('copied')
              setTimeout(() => {
                setCopyState('idle')
              }, 1000)
            })
          }}
          className='absolute top-1 right-1 p-0 w-8 h-8 opacity-0 transition-opacity group-hover:opacity-100'
          variant='ghost'
          size='icon'
          title='Copy to clipboard'
        >
          <Copy className='w-4 h-4' />
        </Button>
      )}
      {copyState === 'copying' && (
        <div className='flex absolute top-1 right-1 justify-center items-center p-0 w-8 h-8'>
          <Loader />
        </div>
      )}
      {copyState === 'copied' && (
        <div className='flex absolute top-1 right-1 z-10 flex-row gap-1 justify-center items-center px-2 h-8 text-green-500 rounded-md bg-muted'>
          <Check className='w-4 h-4' /> Copied!
        </div>
      )}
      {children}
    </div>
  )
}
