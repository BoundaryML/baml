'use server'
import { b } from '../../baml_client'

export async function testAWS(input: string) {
  // auth
  // db

  return b.stream.TestAws(input).toStreamable()
}
