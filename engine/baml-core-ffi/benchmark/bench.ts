import b from 'benny'

import { plus100 } from '../index'

function add(a: number) {
  return a + 100
}

async function run() {
  await b.suite(
    'Add 100',

    b.add('Native a + 100', () => {
      plus100(10)
    }),

    b.add('JavaScript a + 100', () => {
      add(10)
    }),

    b.cycle(),
    b.complete(),
  )
}

run().catch((e) => {
  console.error(e)
})
