'use client'

import { NetworkTimeline } from '@/components/network-timeline'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Loader2 } from 'lucide-react'
import * as React from 'react'
import { useTestAws } from '../../../baml_client/react/hooks'
import type { HookOutput } from '../../../baml_client/react/hooks'

type ResponseCardProps = {
  hookResult: HookOutput<'TestAws'>
  hasStarted: boolean
}

function formatError(error: any): { title: string; message: string; status_code?: number } {
  if (!error) return { title: 'No error', message: 'No error available' }

  try {
    // If error is a string, return it directly
    if (typeof error === 'string') {
      return { title: 'Error', message: error }
    }

    // Parse error if it's a string representation of JSON
    const errorObj = typeof error === 'string' ? JSON.parse(error) : error

    // Extract the most relevant information
    const title = errorObj.name || 'Error'
    let message = errorObj.message || ''

    // If the message contains a nested error structure, try to extract the actual message
    if (message.includes('BamlError:')) {
      // Extract the actual error message from nested structure
      const matches = message.match(/message: Some\(\s*"([^"]+)"\s*\)/)
      if (matches && matches[1]) {
        message = matches[1]
      }
    }

    // Add client name if available
    if (errorObj.client_name) {
      message = `${message}\nClient: ${errorObj.client_name}`
    }

    return {
      title,
      message,
      status_code: errorObj.status_code,
    }
  } catch (e) {
    // Fallback for any parsing errors
    return {
      title: 'Error',
      message: String(error),
    }
  }
}

function ResponseCard({ hookResult, hasStarted }: ResponseCardProps) {
  const { isLoading, error, isError, data, streamData, isPending, isStreaming, isSuccess, finalData } = hookResult

  const dataRef = React.useRef<HTMLPreElement>(null)
  const streamDataRef = React.useRef<HTMLPreElement>(null)
  const finalDataRef = React.useRef<HTMLPreElement>(null)
  const errorRef = React.useRef<HTMLPreElement>(null)
  // Auto-scroll effect for data tab
  React.useEffect(() => {
    if (dataRef.current) {
      dataRef.current.scrollTop = dataRef.current.scrollHeight
    }
  }, [data])

  // Auto-scroll effect for stream data tab
  React.useEffect(() => {
    if (streamDataRef.current) {
      streamDataRef.current.scrollTop = streamDataRef.current.scrollHeight
    }
  }, [streamData])

  // Auto-scroll effect for final data tab
  React.useEffect(() => {
    if (finalDataRef.current) {
      finalDataRef.current.scrollTop = finalDataRef.current.scrollHeight
    }
  }, [finalData])

  return (
    <div className='flex flex-col gap-6'>
      <NetworkTimeline hookResult={hookResult} hasStarted={hasStarted} />

      <div className='space-y-2'>
        <Tabs defaultValue='data' className='w-full'>
          <TabsList className='grid w-full grid-cols-4'>
            <TabsTrigger value='data'>Data</TabsTrigger>
            <TabsTrigger value='streamData'>Stream Data</TabsTrigger>
            <TabsTrigger value='finalData'>Final Data</TabsTrigger>
            <TabsTrigger value='error'>Error</TabsTrigger>
          </TabsList>
          <TabsContent value='data'>
            <pre
              ref={dataRef}
              className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg max-h-[60vh] overflow-y-auto'
            >
              {data ? (typeof data === 'string' ? data : JSON.stringify(data, null, 2)) : 'No data available'}
            </pre>
          </TabsContent>
          <TabsContent value='streamData'>
            <pre
              ref={streamDataRef}
              className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg max-h-[60vh] overflow-y-auto'
            >
              {streamData
                ? typeof streamData === 'string'
                  ? streamData
                  : JSON.stringify(streamData, null, 2)
                : 'No streaming data available'}
            </pre>
          </TabsContent>
          <TabsContent value='finalData'>
            <pre
              ref={finalDataRef}
              className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg max-h-[60vh] overflow-y-auto'
            >
              {finalData
                ? typeof finalData === 'string'
                  ? finalData
                  : JSON.stringify(finalData, null, 2)
                : 'No final data available'}
            </pre>
          </TabsContent>
          <TabsContent value='error'>
            {error ? (
              <div className='space-y-4 max-h-[60vh] overflow-y-auto'>
                <Alert variant='destructive'>
                  <AlertDescription>
                    {(() => {
                      const { title, message, status_code } = formatError(error)
                      return (
                        <div className='space-y-2'>
                          <div className='flex items-center gap-2'>
                            <div className='font-semibold break-words'>{title}</div>
                            {status_code && <Badge variant={'destructive'}>{status_code}</Badge>}
                          </div>
                          <pre className='whitespace-pre-wrap font-mono text-sm break-words'>{message}</pre>
                        </div>
                      )
                    })()}
                  </AlertDescription>
                </Alert>
              </div>
            ) : (
              <pre className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg max-h-[60vh] overflow-y-auto'>
                No error available
              </pre>
            )}
          </TabsContent>
        </Tabs>
      </div>
    </div>
  )
}

export default function TestClient() {
  const [isStreamingEnabled, setIsStreamingEnabled] = React.useState(true)

  const hookResult = useTestAws({
    stream: isStreamingEnabled as true,
    onStreamData: (response) => {},
  })

  const { isLoading, error, isError, isSuccess, mutate, status, data, streamData } = hookResult
  const [prompt, setPrompt] = React.useState('')
  const [hasStarted, setHasStarted] = React.useState(false)

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
      <form onSubmit={handleSubmit} className='space-y-4'>
        <div className='space-y-2'>
          <div className='flex items-center justify-between gap-4'>
            <Label htmlFor='prompt'>Write a story about</Label>
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
              placeholder='A cat in a hat...'
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
