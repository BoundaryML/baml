import { z } from 'zod'
export const bamlConfigSchema = z
  .object({
    cliPath: z.optional(z.string().nullable()).default(null),
    generateCodeOnSave: z.enum(['never', 'always']).default('always'),
    restartTSServerOnSave: z.boolean().default(true),
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
