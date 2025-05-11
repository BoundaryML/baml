import languageServer from './language-server-client'
import type { BamlVSCodePlugin } from './types'

const plugins: BamlVSCodePlugin[] = [languageServer]

export default plugins
