// COPIED FROM ./vscode-ext/packages/language-server/src/bamlConfig.ts

import { workspace } from 'vscode'
import { z } from 'zod'
export const bamlConfigSchema = z
  .object({
    cliPath: z.optional(z.string().nullable()).default(null),
    generateCodeOnSave: z.enum(['never', 'always']).default('always'),
    restartTSServerOnSave: z.boolean().default(false),
    enablePlaygroundProxy: z.boolean().default(true),
    envCommand: z.string().default('env'),
    fileWatcher: z.boolean().default(false),
    trace: z.object({
      server: z.enum(['off', 'messages', 'verbose']).default('off'),
    }),
    bamlPanelOpen: z.boolean().default(false),
  })
  .partial()
type BamlConfig = z.infer<typeof bamlConfigSchema>

export const bamlConfig: { config: BamlConfig | null; cliVersion: string | null } = {
  config: null,
  cliVersion: null,
}

export const getConfig = () => {
  try {
    console.log('getting config')
    const configResponse = workspace.getConfiguration('baml')
    console.log('configResponse ' + JSON.stringify(configResponse, null, 2))
    bamlConfig.config = bamlConfigSchema.parse(configResponse)
  } catch (e: any) {
    if (e instanceof Error) {
      console.log('Error getting config' + e.message + ' ' + e.stack)
    } else {
      console.log('Error getting config' + e)
    }
  }
}
