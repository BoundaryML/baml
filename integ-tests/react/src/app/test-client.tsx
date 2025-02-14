// NOTE: Uncomment this to verify that the types are working
// @ts-nocheck
'use client'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { cn } from '@/lib/utils'
import { Loader2 } from 'lucide-react'
import * as React from 'react'
import { useTestAws } from '../../baml_client/react/hooks'
import type { HookOutput } from '../../baml_client/react/hooks'

type ResponseCardProps = {
  streamingHookResult: HookOutput<'TestAws'>
  nonStreamingHookResult: HookOutput<'TestAws', { stream: false }>
  status: HookOutput<'TestAws', { stream: true }>['status']
}

function StatusBadge({ status }: { status: HookOutput<'TestAws', { stream: true }>['status'] }) {
  return (
    <div className='flex flex-col gap-2 items-center justify-center'>
      <span className='font-medium text-foreground'>Status</span>
      <span
        className={cn(
          'font-normal text-muted-foreground',
          {
            idle: 'text-gray-500',
            pending: 'text-yellow-500',
            success: 'text-green-500',
            error: 'text-red-500',
            streaming: 'text-blue-500',
          }[status],
        )}
      >
        {status}
      </span>
    </div>
  )
}

function BooleanBadge({ label, value }: { label: string; value: boolean }) {
  return (
    <div className='flex flex-col gap-2 items-center justify-center'>
      <span className='font-medium text-foreground'>{label}</span>
      <span className='font-medium text-foreground'>{value ? '✅' : '❌'}</span>
    </div>
  )
}

function ResponseCard({ streamingHookResult, nonStreamingHookResult, status }: ResponseCardProps) {
  const { isLoading, error, isError, data, streamData, isPending, isStreaming, isSuccess, finalData } =
    streamingHookResult

  return (
    <div className='flex flex-col gap-4'>
      {isError && (
        <Alert variant='destructive' className='mt-4'>
          <AlertDescription>Error: {error?.message}</AlertDescription>
        </Alert>
      )}
      <div className='text-sm text-muted-foreground text-center gap-4 flex justify-between'>
        <StatusBadge status={status} />
        <BooleanBadge label='IsLoading' value={isLoading} />
        <BooleanBadge label='IsError' value={isError} />
        <BooleanBadge label='IsSuccess' value={isSuccess} />
        <BooleanBadge label='IsPending' value={isPending} />
        <BooleanBadge label='IsStreaming' value={isStreaming} />
      </div>

      <div className='mt-6 space-y-2'>
        <Tabs defaultValue='data' className='w-full'>
          <TabsList className='grid w-full grid-cols-3'>
            <TabsTrigger value='data'>Data</TabsTrigger>
            <TabsTrigger value='streamData'>Stream Data</TabsTrigger>
            <TabsTrigger value='finalData'>Final Data</TabsTrigger>
          </TabsList>
          <TabsContent value='data'>
            <pre className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg'>
              {data ? (typeof data === 'string' ? data : JSON.stringify(data, null, 2)) : 'No data available'}
            </pre>
          </TabsContent>
          <TabsContent value='streamData'>
            <pre className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg'>
              {streamData
                ? typeof streamData === 'string'
                  ? streamData
                  : JSON.stringify(streamData, null, 2)
                : 'No streaming data available'}
            </pre>
          </TabsContent>
          <TabsContent value='finalData'>
            <pre className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg'>
              {finalData
                ? typeof finalData === 'string'
                  ? finalData
                  : JSON.stringify(finalData, null, 2)
                : 'No final data available'}
            </pre>
          </TabsContent>
        </Tabs>
      </div>
    </div>
  )
}

export default function TestClient() {
  const streamingDirectAction = useTestAws({
    stream: true,
    onStreamData: (response) => {
      console.log('Got partial response')
    },
    onFinalData: (response) => {
      console.log('Got final response')
    },
    onData: (response) => {
      console.log('Got data')
    },
    onError: (error) => {
      console.error('Got error', error)
    },
  })

  // // Streaming should not have errors
  streamingDirectAction satisfies HookOutput<'TestAws', { stream: true }>
  streamingDirectAction.data satisfies string | undefined
  streamingDirectAction.streamData satisfies string | undefined
  streamingDirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // // Non-Streaming should have errors
  streamingDirectAction satisfies HookOutput<'TestAws'>
  streamingDirectAction.data satisfies never
  streamingDirectAction.streamData satisfies never
  streamingDirectAction.mutate satisfies (input: string) => Promise<string>

  const explicitNonStreamingDirectAction = useTestAws({
    stream: false,
    onFinalData: (response) => {
      console.log('Got final response', response)
    },
    onError: (error) => {
      console.error('Got error', error)
    },
  })

  // Streaming should have errors
  explicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: true }>
  explicitNonStreamingDirectAction.data satisfies never
  explicitNonStreamingDirectAction.streamData satisfies never
  explicitNonStreamingDirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // Non-Streaming should not have errors
  explicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: false }>
  explicitNonStreamingDirectAction.data satisfies string | undefined
  explicitNonStreamingDirectAction.streamData satisfies undefined
  explicitNonStreamingDirectAction.mutate satisfies (input: string) => Promise<string>

  const nonExplicitNonStreamingDirectAction = useTestAws()

  // Streaming should have errors
  nonExplicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: true }>
  nonExplicitNonStreamingDirectAction.data satisfies string | undefined
  nonExplicitNonStreamingDirectAction.streamData satisfies string | undefined
  nonExplicitNonStreamingDirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // Non-Streaming should not have errors
  nonExplicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: false }>
  nonExplicitNonStreamingDirectAction.data satisfies never
  nonExplicitNonStreamingDirectAction.streamData satisfies never
  nonExplicitNonStreamingDirectAction.mutate satisfies (input: string) => Promise<string>

  // const streamingIndirectAction = useBamlAction(TestAws, {
  //   stream: true,
  //   onPartial: (response) => {
  //     console.log('Got partial response', response)
  //   },
  //   onFinal: (response) => {
  //     console.log('Got final response', response)
  //   },
  //   onError: (error) => {
  //     console.error('Got error', error)
  //   },
  // })

  // // Streaming should not have errors
  // streamingIndirectAction satisfies StreamingHookResult<'TestAws'>
  // streamingIndirectAction.data satisfies string | undefined
  // streamingIndirectAction.streamingData satisfies string | null | undefined
  // streamingIndirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // // Non-Streaming should have errors
  // streamingIndirectAction satisfies NonStreamingHookResult<'TestAws'>
  // streamingIndirectAction.data satisfies never
  // streamingIndirectAction.streamingData satisfies never | undefined
  // streamingIndirectAction.mutate satisfies (input: string) => Promise<string>

  // const nonStreamingIndirectAction = useBamlAction(TestAws, {
  //   onFinal: (response) => {
  //     console.log('Got final response', response)
  //   },
  //   onError: (error) => {
  //     console.error('Got error', error)
  //   },
  // })

  // // Streaming should have errors
  // nonStreamingIndirectAction satisfies StreamingHookResult<'TestAws'>
  // nonStreamingIndirectAction.data satisfies never
  // nonStreamingIndirectAction.streamingData satisfies never
  // nonStreamingIndirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // // Non-Streaming should not have errors
  // nonStreamingIndirectAction satisfies NonStreamingHookResult<'TestAws'>
  // nonStreamingIndirectAction.data satisfies string | undefined
  // nonStreamingIndirectAction.streamingData satisfies never | undefined
  // nonStreamingIndirectAction.mutate satisfies (input: string) => Promise<string>

  const { isLoading, error, isError, isSuccess, mutate, status, data, streamData } = streamingDirectAction

  const [prompt, setPrompt] = React.useState('')

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!prompt.trim()) return

    await mutate(prompt)
    setPrompt('')
  }

  return (
    <Card className='w-full'>
      <CardHeader>
        <CardTitle>BAML AWS Test</CardTitle>
        <CardDescription>Test the BAML AWS integration by entering some text below.</CardDescription>
      </CardHeader>

      <CardContent className='flex flex-col gap-4'>
        <form onSubmit={handleSubmit} className='space-y-4'>
          <div className='space-y-2'>
            <Label htmlFor='prompt'>Test Input</Label>
            <Input
              id='prompt'
              type='text'
              value={prompt}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => setPrompt(e.target.value)}
              placeholder='Type something...'
              disabled={isLoading}
            />
          </div>

          <Button type='submit' className='w-full' disabled={isLoading || !prompt.trim()}>
            {isLoading && <Loader2 className='mr-2 h-4 w-4 animate-spin' />}
            {isLoading ? 'Processing...' : 'Submit'}
          </Button>
        </form>

        <ResponseCard
          streamingHookResult={streamingDirectAction}
          nonStreamingHookResult={explicitNonStreamingDirectAction}
          status={status}
        />
      </CardContent>
    </Card>
  )
}
