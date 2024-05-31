import { lezer } from '@lezer/generator/rollup'
import typescript from 'rollup-plugin-ts'

export default {
  input: 'src/index.ts',
  external: (id) => id != 'tslib' && !/^(\.?\/|\w:)/.test(id),
  output: [
    { file: 'dist/index.cjs', format: 'cjs' },
    { dir: './dist', format: 'es' },
  ],
  plugins: [lezer(), typescript()],
}
