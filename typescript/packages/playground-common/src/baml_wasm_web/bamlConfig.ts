// COPIED FROM ./vscode-ext/packages/vscode/src/plugins/language-server/bamlConfig.ts

import { atom } from 'jotai';
import { z } from 'zod';
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
    syncExtensionToGeneratorVersion: z
      .enum(['auto', 'never', 'always'])
      .default('auto'),
    featureFlags: z.array(z.enum(['beta', 'display_all_warnings'])).default([]),
  })
  .partial();
type BamlConfig = z.infer<typeof bamlConfigSchema>;

export type BamlConfigAtom = {
  config: BamlConfig | null;
  cliVersion: string | null;
};

export const bamlConfig = atom<BamlConfigAtom>({
  config: null,
  cliVersion: null,
});
