import test from 'ava'

import { isAvailable } from '../index'

test('sync function from native code', (t) => {
  t.is(isAvailable(), true)
})
