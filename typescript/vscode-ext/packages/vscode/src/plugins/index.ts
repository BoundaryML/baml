import languageServer from './language-server'
import type { BamlVSCodePlugin } from './types'

const plugins: BamlVSCodePlugin[] = [languageServer]

export default plugins
