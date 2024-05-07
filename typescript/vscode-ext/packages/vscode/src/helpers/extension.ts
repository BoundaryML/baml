import { type ExtensionContext, ExtensionMode } from 'vscode'

export class Extension {
  private static instance: Extension

  private constructor(private ctx: ExtensionContext) {}

  public static getInstance(ctx?: ExtensionContext): Extension {
    if (!Extension.instance && ctx) {
      Extension.instance = new Extension(ctx)
    }

    return Extension.instance
  }

  /**
   * Check if the extension is in production/development mode
   */
  public get isProductionMode(): boolean {
    return this.ctx.extensionMode === ExtensionMode.Production
  }
}
