'use client'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { Loader2 } from 'lucide-react'
import * as React from 'react'
import { useAtom } from 'jotai'
import {  selectedExampleAtom, getHookConfig } from '@/lib/store'
import { ResponseCard } from './response-card'

export function TestClient() {
  const [selectedExample] = useAtom(selectedExampleAtom)
  const [isStreamingEnabled, setIsStreamingEnabled] = React.useState(true)

  // Get the hook config for the selected example
  const hookConfig = getHookConfig(selectedExample)

  // Create a memoized hook to avoid re-rendering issues
  const CurrentHook = React.useMemo(() => hookConfig.hook, [hookConfig?.hook])

  // Use the hook dynamically based on the selected example
  const hookResult = CurrentHook({
    stream: isStreamingEnabled as true,
    onStreamData: (response: any) => {},
  })

  const { isLoading, error, isError, isSuccess, mutate, status, data, streamData } = hookResult
  const [prompt, setPrompt] = React.useState('')
  const [hasStarted, setHasStarted] = React.useState(false)

  // Reset prompt when example changes
  React.useEffect(() => {
    setPrompt('')
    setHasStarted(false)
    if (hookResult.reset) {
      hookResult.reset()
    }
  }, [selectedExample])

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!prompt.trim()) return

    setHasStarted(true)
    await mutate(prompt)
  }

  // Reset hasStarted when the request is complete or reset
  React.useEffect(() => {
    if (!isLoading && !streamData && !data && !error) {
      setHasStarted(false)
    }
  }, [isLoading, streamData, data, error])

  return (
    <div className='flex flex-col gap-6 w-full'>
      <div className='text-center mb-4'>
        <h2 className='text-xl font-medium'>{hookConfig?.description}</h2>
      </div>

      <form onSubmit={handleSubmit} className='space-y-4'>
        <div className='space-y-2'>
          <div className='flex items-center justify-between gap-4'>
            <Label htmlFor='prompt'>{hookConfig?.inputLabel}</Label>
            <div className='flex items-center space-x-2'>
              <Label htmlFor='streaming-switch' className='text-sm text-muted-foreground'>
                Stream Response
              </Label>
              <Switch
                id='streaming-switch'
                checked={isStreamingEnabled}
                onCheckedChange={setIsStreamingEnabled}
                aria-label='Toggle streaming'
              />
            </div>
          </div>
          <div className='flex items-center gap-4'>
            <Input
              id='prompt'
              type='text'
              value={prompt}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => setPrompt(e.target.value)}
              placeholder={hookConfig?.inputPlaceholder}
              disabled={isLoading}
            />
            <div className='flex items-center justify-between space-x-2'>
              {!isSuccess && !isError && (
                <Button type='submit' disabled={isLoading || !prompt.trim()} className='flex-1 min-w-40'>
                  {isLoading && <Loader2 className='mr-2 h-4 w-4 animate-spin' />}
                  {isLoading ? 'Processing...' : 'Submit'}
                </Button>
              )}
              {(isSuccess || isError) && (
                <Button
                  variant='outline'
                  className='flex-1 min-w-40'
                  disabled={isLoading}
                  onClick={() => {
                    setHasStarted(false)
                    hookResult.reset()
                  }}
                >
                  Reset
                </Button>
              )}
            </div>
          </div>
        </div>
      </form>

      <ResponseCard hookResult={hookResult} hasStarted={hasStarted} />
    </div>
  )
}
