import type { Connection } from 'vscode-languageserver'

export type BlockType = 'generator' | 'datasource' | 'model' | 'type' | 'enum' | 'view'

export interface LSOptions {
  /**
   * If you have a connection already that the ls should use, pass it in.
   * Else the connection will be created from `process`.
   */
  connection?: Connection
}

// eslint-disable-next-line @typescript-eslint/no-empty-interface
export type LSSettings = {}

export interface BAMLMessage {
  type: 'warn' | 'info' | 'error'
  message: string
}
