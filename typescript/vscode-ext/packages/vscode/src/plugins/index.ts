import languageServer from './language-server'
import { BamlVSCodePlugin } from './types'

const plugins: BamlVSCodePlugin[] = [languageServer]

export default plugins
