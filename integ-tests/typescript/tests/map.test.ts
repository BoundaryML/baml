import { b } from './test-setup'
import { MapKey } from '../baml_client'

describe('Map tests', () => {
  it('enum key in map', async () => {
    const res = await b.InOutEnumMapKey({ [MapKey.A]: 'A' }, { [MapKey.B]: 'B' })
    expect(res).toHaveProperty(MapKey.A, 'A')
    expect(res).toHaveProperty(MapKey.B, 'B')
  })
})
