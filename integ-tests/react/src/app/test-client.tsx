// NOTE: Uncomment this to verify that the types are working
// @ts-nocheck
'use client'

import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Loader2 } from 'lucide-react'
import * as React from 'react'
import { useTestAws } from '../../baml_client/react/hooks'
import type { HookOutput } from '../../baml_client/react/hooks'

type ResponseCardProps = {
  streamingHookResult: HookOutput<'TestAws'>
  nonStreamingHookResult: HookOutput<'TestAws', { stream: false }>
  status: HookOutput<'TestAws', { stream: true }>['status']
}

function ResponseCard({ streamingHookResult, nonStreamingHookResult, status }: ResponseCardProps) {
  const { isLoading, error, isError, data, streamingData } = streamingHookResult
  const response = isLoading ? streamingData : data

  return (
    <>
      {isError && (
        <Alert variant='destructive' className='mt-4'>
          <AlertDescription>Error: {error?.message}</AlertDescription>
        </Alert>
      )}

      {response && (
        <div className='mt-6 space-y-2'>
          <Card>
            <CardContent className='pt-6'>
              <pre className='whitespace-pre-wrap font-mono text-sm bg-muted p-4 rounded-lg'>
                {typeof response === 'string' ? response : JSON.stringify(response, null, 2)}
              </pre>
            </CardContent>
          </Card>
        </div>
      )}
      <CardFooter className='text-sm text-muted-foreground text-center'>{status}</CardFooter>
    </>
  )
}

export default function TestClient() {
  const streamingDirectAction = useTestAws({
    stream: true,
    onPartial: (response) => {
      console.log('Got partial response', response)
    },
    onFinal: (response) => {
      console.log('Got final response', response)
    },
    onError: (error) => {
      console.error('Got error', error)
    },
  })

  // // Streaming should not have errors
  streamingDirectAction satisfies HookOutput<'TestAws', { stream: true }>
  streamingDirectAction.data satisfies string | undefined
  streamingDirectAction.streamingData satisfies string | undefined
  streamingDirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // // Non-Streaming should have errors
  streamingDirectAction satisfies HookOutput<'TestAws'>
  streamingDirectAction.data satisfies never
  streamingDirectAction.streamingData satisfies never
  streamingDirectAction.mutate satisfies (input: string) => Promise<string>

  const explicitNonStreamingDirectAction = useTestAws({
    stream: false,
    onFinal: (response) => {
      console.log('Got final response', response)
    },
    onError: (error) => {
      console.error('Got error', error)
    },
  })

  // Streaming should have errors
  explicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: true }>
  explicitNonStreamingDirectAction.data satisfies never
  explicitNonStreamingDirectAction.streamingData satisfies never
  explicitNonStreamingDirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // Non-Streaming should not have errors
  explicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: false }>
  explicitNonStreamingDirectAction.data satisfies string | undefined
  explicitNonStreamingDirectAction.streamingData satisfies undefined
  explicitNonStreamingDirectAction.mutate satisfies (input: string) => Promise<string>

  const nonExplicitNonStreamingDirectAction = useTestAws({
    // stream: undefined,
    onFinal: (response) => {
      console.log('Got final response', response)
    },
    onError: (error) => {
      console.error('Got error', error)
    },
  })

  // Streaming should have errors
  nonExplicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: true }>
  nonExplicitNonStreamingDirectAction.data satisfies string | undefined
  nonExplicitNonStreamingDirectAction.streamingData satisfies string | undefined
  nonExplicitNonStreamingDirectAction.mutate satisfies (input: string) => Promise<ReadableStream<Uint8Array>>

  // Non-Streaming should not have errors
  nonExplicitNonStreamingDirectAction satisfies HookOutput<'TestAws', { stream: false }>
  nonExplicitNonStreamingDirectAction.data satisfies never
  nonExplicitNonStreamingDirectAction.streamingData satisfies never
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

  const { isLoading, error, isError, isSuccess, mutate, status, data, streamingData } = streamingDirectAction

  const response = isLoading ? streamingData : data
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

      <CardContent>
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
