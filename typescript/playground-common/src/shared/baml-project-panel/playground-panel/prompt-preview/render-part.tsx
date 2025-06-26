import type { WasmChatMessagePart, WasmParam, WasmTestCase } from '@gloo-ai/baml-schema-wasm-web'
import he from 'he'
import { RenderPromptPart } from './render-text'
import { WebviewMedia } from './webview-media'

export const RenderPart: React.FC<{
  part: WasmChatMessagePart
  testCase?: WasmTestCase
}> = ({ part, testCase }) => {
  if (part.is_text()) {
    const extractStringValues = (inputs: WasmParam[]): string[] => {
      if (!inputs || !Array.isArray(inputs)) return []
      return inputs.flatMap((input) => {
        if (typeof input.value === 'string') {
          try {
            // Try to parse the string as JSON
            const parsed = JSON.parse(input.value)
            if (typeof parsed === 'object') {
              return Object.values(parsed).filter((val): val is string => typeof val === 'string')
            } else {
              return [he.encode(parsed)]
            }
          } catch {
            // If parsing fails, treat it as a regular string
            return [he.encode(input.value)]
          }
        }
        if (typeof input.value === 'object') {
          return Object.values(input.value).filter((val): val is string => typeof val === 'string')
        }
        return []
      })
    }

    // this makes it so that we can escape html
    const text = he.encode(part.as_text() ?? '')
    // Skip processing if any input value is too large
    const hasLargeInput = (testCase?.inputs ?? []).some(
      (input) => typeof input.value === 'string' && input.value.length > 20000,
    )

    const allChunks = hasLargeInput ? [] : extractStringValues(testCase?.inputs ?? [])
    const highlightChunks = allChunks.filter((chunk) => {
      if (!chunk || !text) return false
      try {
        // Escape special regex characters in the chunk
        const escapedChunk = chunk.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
        // Use unicode flag to handle emojis correctly
        const regex = new RegExp(escapedChunk, 'gu')
        const matches = text.match(regex)
        // Only include chunks that appear at least once in the text
        return matches && matches.length === 1
      } catch (e) {
        console.error('Error highlighting text part', e)
        return false
      }
    })

    return text ? <RenderPromptPart text={text} highlightChunks={highlightChunks} /> : null
  }

  const media = part.as_media()
  if (!media) {
    return null
  }

  if (part.is_image()) {
    return <WebviewMedia bamlMediaType='image' media={media} />
  }

  if (part.is_audio()) {
    return <WebviewMedia bamlMediaType='audio' media={media} />
  }

  return null
}
