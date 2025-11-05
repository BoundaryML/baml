import { config } from 'dotenv'
import { ClientRegistry, BamlValidationError } from '@boundaryml/baml'
import { b } from '../baml_client'
import { b as b_sync } from '../baml_client/sync_client'
import { DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME, resetBamlEnvVars } from '../baml_client/globals'
import { ReadableStream, ReadableStreamDefaultController } from 'node:stream/web'
import { TextEncoder, TextDecoder } from 'util'
import '@testing-library/jest-dom'

config()

beforeAll(() => {
  // Add any global setup here
})

afterAll(() => {
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
})

// Add web stream APIs to global scope for tests
Object.assign(global, {
  ReadableStream,
  ReadableStreamDefaultController,
  TextEncoder,
  TextDecoder,
})


export {
  b,
  b_sync,
  ClientRegistry,
  BamlValidationError,
  resetBamlEnvVars,
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
}
