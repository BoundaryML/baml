import test from 'ava'
import * as pkg from '../package.json'

import { version } from '../index'

test('sync function from native code', (t) => {
  t.is(version(), pkg.version)
})
