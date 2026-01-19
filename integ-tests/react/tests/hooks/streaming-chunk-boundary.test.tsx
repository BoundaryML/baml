/**
 * Test for streaming chunk boundary handling.
 *
 * This test verifies that the React hooks correctly handle cases where:
 * 1. Multiple JSON objects arrive in a single chunk
 * 2. A single JSON object is split across multiple chunks
 *
 * These scenarios occur due to TCP buffering differences between
 * localhost (dev) and production (Vercel) environments.
 */
import { render, waitFor } from '@testing-library/react'
import { act, createRef, forwardRef, useEffect, useImperativeHandle } from 'react'
import { TextEncoder } from 'util'

import {
  type HookInput,
  type HookOutput,
  useAaaSamOutputFormat,
} from '../../baml_client/react/hooks'
import * as StreamingActions from '../../baml_client/react/server_streaming'

type StreamingHookOutput = HookOutput<'AaaSamOutputFormat', { stream: true }>

type HookHarnessProps = {
  options: HookInput<'AaaSamOutputFormat', { stream: true }>
  onStateChange: (state: StreamingHookOutput) => void
}

type HookHarnessHandle = {
  mutate: StreamingHookOutput['mutate']
}

const HookHarness = forwardRef<HookHarnessHandle, HookHarnessProps>(({ options, onStateChange }, ref) => {
  const streamingState = useAaaSamOutputFormat(options) as StreamingHookOutput

  useEffect(() => {
    onStateChange(streamingState)
  }, [onStateChange, streamingState])

  useImperativeHandle(
    ref,
    () => ({ mutate: streamingState.mutate }),
    [streamingState.mutate],
  )

  return null
})

/**
 * Creates a ReadableStream that delivers chunks with specified boundaries.
 * This simulates real-world TCP buffering behavior.
 */
function createChunkedStream(chunks: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder()
  let chunkIndex = 0

  return new ReadableStream({
    pull(controller) {
      if (chunkIndex < chunks.length) {
        controller.enqueue(encoder.encode(chunks[chunkIndex]))
        chunkIndex++
      } else {
        controller.close()
      }
    },
  })
}

describe('streaming chunk boundary handling', () => {
  const partial1 = { ingredients: { Flour: { amount: 1 } } }
  const partial2 = { ingredients: { Flour: { amount: 1, unit: 'cup' } } }
  const finalResult = {
    ingredients: { Flour: { amount: 1, unit: 'cup' }, Eggs: { amount: 2 } },
    recipe_type: 'dinner' as const,
  }

  let originalAction: typeof StreamingActions.AaaSamOutputFormat

  beforeEach(() => {
    originalAction = StreamingActions.AaaSamOutputFormat
  })

  afterEach(() => {
    // Restore original action
    ;(StreamingActions as any).AaaSamOutputFormat = originalAction
  })

  it('handles normal single-message-per-chunk behavior (happy path)', async () => {
    // This is the "happy path" - one NDJSON message per chunk
    const chunks = [
      JSON.stringify({ partial: partial1 }) + '\n',
      JSON.stringify({ partial: partial2 }) + '\n',
      JSON.stringify({ final: finalResult }) + '\n',
    ]

    ;(StreamingActions as any).AaaSamOutputFormat = jest.fn(() =>
      Promise.resolve(createChunkedStream(chunks))
    )

    const onStreamData = jest.fn()
    const onFinalData = jest.fn()
    let latestState: StreamingHookOutput | undefined
    const harnessRef = createRef<HookHarnessHandle>()

    render(
      <HookHarness
        ref={harnessRef}
        options={{
          stream: true,
          onStreamData,
          onFinalData,
        }}
        onStateChange={state => {
          latestState = state
        }}
      />,
    )

    await act(async () => {
      await harnessRef.current?.mutate('recipe input')
    })

    await waitFor(() => {
      expect(latestState?.status).toBe('success')
    }, { timeout: 5000 })

    expect(onStreamData).toHaveBeenCalledTimes(2)
    expect(onFinalData).toHaveBeenCalledWith(finalResult)
  })

  it('handles multiple JSON objects in a single chunk (NDJSON)', async () => {
    // Simulate TCP buffering that delivers multiple messages at once
    // With NDJSON format, each message ends with \n
    const combinedChunk =
      JSON.stringify({ partial: partial1 }) + '\n' +
      JSON.stringify({ partial: partial2 }) + '\n' +
      JSON.stringify({ final: finalResult }) + '\n'

    ;(StreamingActions as any).AaaSamOutputFormat = jest.fn(() =>
      Promise.resolve(createChunkedStream([combinedChunk]))
    )

    const onStreamData = jest.fn()
    const onFinalData = jest.fn()
    const onError = jest.fn()
    let latestState: StreamingHookOutput | undefined
    const harnessRef = createRef<HookHarnessHandle>()

    render(
      <HookHarness
        ref={harnessRef}
        options={{
          stream: true,
          onStreamData,
          onFinalData,
          onError,
        }}
        onStateChange={state => {
          latestState = state
        }}
      />,
    )

    await act(async () => {
      await harnessRef.current?.mutate('recipe input')
    })

    // With NDJSON fix, this should now succeed
    await waitFor(() => {
      expect(latestState?.status).toBe('success')
    }, { timeout: 5000 })

    // All callbacks should be called correctly
    expect(onStreamData).toHaveBeenCalledWith(partial1)
    expect(onStreamData).toHaveBeenCalledWith(partial2)
    expect(onFinalData).toHaveBeenCalledWith(finalResult)
    expect(onError).not.toHaveBeenCalled()
  })

  it('handles JSON object split across multiple chunks (NDJSON)', async () => {
    // Simulate TCP buffering that splits a JSON object mid-way
    // With NDJSON, messages end with \n so we can buffer incomplete messages
    const fullJson = JSON.stringify({ partial: partial1 }) + '\n'
    const splitPoint = Math.floor(fullJson.length / 2)
    const chunk1 = fullJson.slice(0, splitPoint)
    const chunk2 = fullJson.slice(splitPoint) + JSON.stringify({ final: finalResult }) + '\n'

    console.log('Split test - chunk1:', chunk1)
    console.log('Split test - chunk2:', chunk2)

    ;(StreamingActions as any).AaaSamOutputFormat = jest.fn(() =>
      Promise.resolve(createChunkedStream([chunk1, chunk2]))
    )

    const onStreamData = jest.fn()
    const onFinalData = jest.fn()
    const onError = jest.fn()
    let latestState: StreamingHookOutput | undefined
    const harnessRef = createRef<HookHarnessHandle>()

    render(
      <HookHarness
        ref={harnessRef}
        options={{
          stream: true,
          onStreamData,
          onFinalData,
          onError,
        }}
        onStateChange={state => {
          latestState = state
        }}
      />,
    )

    await act(async () => {
      await harnessRef.current?.mutate('recipe input')
    })

    // With NDJSON fix, this should now succeed - buffer handles split messages
    await waitFor(() => {
      expect(latestState?.status).toBe('success')
    }, { timeout: 5000 })

    // All callbacks should be called correctly
    expect(onStreamData).toHaveBeenCalledWith(partial1)
    expect(onFinalData).toHaveBeenCalledWith(finalResult)
    expect(onError).not.toHaveBeenCalled()
  })
})
