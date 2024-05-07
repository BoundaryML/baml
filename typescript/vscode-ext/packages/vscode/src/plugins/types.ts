import type { ExtensionContext, OutputChannel } from 'vscode'

export interface BamlVSCodePlugin {
  name: string
  /** This is called during vscodes' activate event and if true will call the plugins activate function */
  enabled: () => Promise<boolean> | boolean

  /** Called when the extension is activated and if enabled returns true */
  activate?: (context: ExtensionContext, outputChannel: OutputChannel) => Promise<void> | void
  deactivate?: () => Promise<void> | void
}
