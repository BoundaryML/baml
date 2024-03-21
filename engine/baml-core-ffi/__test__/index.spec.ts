import test from 'ava'

import { version } from '../index'

import * as pkg from '../package.json'

test('sync function from native code', (t) => {
  t.is(version(), pkg.version)
})
