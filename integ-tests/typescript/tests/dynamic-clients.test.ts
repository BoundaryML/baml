import { b } from './test-setup'
import { ClientRegistry } from '@boundaryml/baml'

describe('Dynamic Clients', () => {
  it('should work with dynamic client', async () => {
    const clientRegistry = new ClientRegistry()
    clientRegistry.addLlmClient('myClient', 'openai', {
      model: 'gpt-3.5-turbo',
    })
    clientRegistry.setPrimary('myClient')

    const capitol = await b.ExpectFailure({
      clientRegistry,
    })
    expect(capitol.toLowerCase()).toContain('london')
  })

})
