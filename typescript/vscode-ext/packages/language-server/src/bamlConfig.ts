import { z } from 'zod'
export const bamlConfigSchema = z
  .object({
    versionCheckMode: z.optional(z.enum(['strict', 'allow-newer'])).default('allow-newer'),
    cliPath: z.optional(z.string().nullable()).default(null),
    generateCodeOnSave: z.enum(['never', 'always', 'same-version-only', 'use-cli-path']).default('always'),
    envCommand: z.string().default('env'),
    fileWatcher: z.boolean().default(false),
    trace: z.object({
      server: z.enum(['off', 'messages', 'verbose']).default('off'),
    }),
    bamlPanelOpen: z.boolean().default(false),
  })
  .partial()
type BamlConfig = z.infer<typeof bamlConfigSchema>
let config: BamlConfig | null = null

export const bamlConfig: { config: BamlConfig | null } = { config: null }
