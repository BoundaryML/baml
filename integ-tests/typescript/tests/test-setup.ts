import { config } from 'dotenv'
import { ClientRegistry, BamlValidationError, BamlClientHttpError } from '@boundaryml/baml'
import { b } from '../baml_client'
import { b as b_sync } from '../baml_client/sync_client'
import { DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME, resetBamlEnvVars } from '../baml_client/globals'

config()

beforeAll(() => {
  // Add any global setup here
})

afterAll(() => {
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
})

export {
  b,
  b_sync,
  ClientRegistry,
  BamlValidationError,
  BamlClientHttpError,
  resetBamlEnvVars,
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
}
