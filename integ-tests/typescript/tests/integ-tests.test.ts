import { b } from '../baml_client'

describe('Integ tests', () => {
  it('should run integ tests', async () => {
    b.ClassifyMessage("hello")

  })
})
