'use client'

import { NetworkTimeline } from '@/components/network-timeline'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { formatError } from './format-error'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import * as React from 'react'
import type { FunctionNames, HookOutput } from '../../../baml_client/react/hooks'

type ResponseCardProps = {
  hookResult: HookOutput<FunctionNames>
  hasStarted: boolean
}
export function ResponseCard({ hookResult, hasStarted }: ResponseCardProps) {
  const { isLoading, error, isError, data, streamData, isPending, isStreaming, isSuccess, finalData } = hookResult

  const dataRef = React.useRef<HTMLPreElement>(null)
  const streamDataRef = React.useRef<HTMLPreElement>(null)
  const finalDataRef = React.useRef<HTMLPreElement>(null)

  // Add state to track the active tab
  const [activeTab, setActiveTab] = React.useState('data')

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

  React.useEffect(() => {
    if (error) {
      // Automatically switch to the error tab when an error occurs
      setActiveTab('error')
    }
  }, [error])

  return (
    <div className='flex flex-col gap-6'>
      <NetworkTimeline hookResult={hookResult} hasStarted={hasStarted} />

      <div className='space-y-2'>
        <Tabs value={activeTab} onValueChange={setActiveTab} className='w-full'>
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
                            {status_code && <Badge variant='destructive'>{status_code}</Badge>}
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
