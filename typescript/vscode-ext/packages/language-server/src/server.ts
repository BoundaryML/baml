import {
  CodeActionKind,
  CodeActionParams,
  CodeLens,
  type CodeLensParams,
  Command,
  CompletionItem,
  CompletionParams,
  type Connection,
  type DeclarationParams,
  Diagnostic,
  DidChangeConfigurationNotification,
  DidChangeWatchedFilesNotification,
  DocumentFormattingParams,
  type DocumentSymbolParams,
  FileSystemWatcher,
  type HoverParams,
  type InitializeParams,
  type InitializeResult,
  Position,
  Range,
  RenameParams,
  TextDocumentSyncKind,
  TextDocuments,
} from 'vscode-languageserver'
import { URI } from 'vscode-uri'

import debounce from 'lodash/debounce'
import { TextDocument } from 'vscode-languageserver-textdocument'
import { IPCMessageReader, IPCMessageWriter, createConnection } from 'vscode-languageserver/node'
import { getWordAtPosition } from './lib/ast'
// import { FileChangeType } from 'vscode'
import fs from 'fs'
// import { cliBuild, cliCheckForUpdates, cliVersion } from './baml-cli'
import { type ParserDatabase, TestRequest } from '@baml/common'
import { z } from 'zod'
// import { getVersion, getEnginesVersion } from './lib/wasm/internals'
import BamlProjectManager, { GeneratorDisabledReason, GeneratorStatus, GeneratorType } from './lib/baml_project_manager'
import type { LSOptions, LSSettings } from './lib/types'
import { BamlWasm } from './lib/wasm'
import { bamlConfig, bamlConfigSchema } from './bamlConfig'
import { cliBuild } from './baml-cli'
import { exec } from 'child_process'

try {
  // only required on vscode versions 1.89 and below.
  ;(globalThis as any).crypto = require('node:crypto').webcrypto
} catch (e) {
  console.log('cant load webcrypto', e)
}

const packageJson = require('../../package.json') // eslint-disable-line
function getConnection(options?: LSOptions): Connection {
  let connection = options?.connection
  if (!connection) {
    connection = process.argv.includes('--stdio')
      ? createConnection(process.stdin, process.stdout)
      : createConnection(new IPCMessageReader(process), new IPCMessageWriter(process))
  }
  return connection
}

let hasCodeActionLiteralsCapability = false
let hasConfigurationCapability = true

/**
 * Starts the language server.
 *
 * @param options Options to customize behavior
 */
export function startServer(options?: LSOptions): void {
  console.log('Server-side -- startServer()')
  // Source code: https://github.com/microsoft/vscode-languageserver-node/blob/main/server/src/common/server.ts#L1044
  const connection: Connection = getConnection(options)
  const bamlProjectManager = new BamlProjectManager((params) => {
    switch (params.type) {
      case 'runtime_updated':
        // console.log(`runtime_updated today! ${Object.keys(params.files).length}: ${Object.entries(params.files).length}`)
        connection.sendRequest('runtime_updated', params)
        break
      case 'diagnostic':
        params.errors.forEach(([uri, diagnostics]) => {
          connection.sendDiagnostics({ uri: uri, diagnostics: diagnostics })
        })

        // Determine number of warnings and errors
        const errors = params.errors.reduce((acc, [, diagnostics]) => {
          return acc + diagnostics.filter((d) => d.severity === 1).length
        }, 0)
        const warnings = params.errors.reduce((acc, [, diagnostics]) => {
          return acc + diagnostics.filter((d) => d.severity === 2).length
        }, 0)
        connection.sendRequest('runtime_diagnostics', { errors, warnings })
        break
      case 'error':
      case 'warn':
      case 'info':
        connection.sendNotification('baml/message', { message: params.message, type: params.type ?? 'warn' })
        break
      default:
        console.error(`Unknown notification type ${params}`)
    }
  })

  console.log = connection.console.log.bind(connection.console)
  console.error = connection.console.error.bind(connection.console)

  console.log('Starting Baml Language Server...')

  const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument)

  connection.onInitialize((params: InitializeParams) => {
    connection.console.info(
      // eslint-disable-next-line
      `Extension '${packageJson?.name}': ${packageJson?.version}`,
    )
    // ... and then capabilities of the language server
    const capabilities = params.capabilities

    hasCodeActionLiteralsCapability = Boolean(capabilities?.textDocument?.codeAction?.codeActionLiteralSupport)
    hasConfigurationCapability = Boolean(capabilities?.workspace?.configuration)

    const result: InitializeResult = {
      capabilities: {
        textDocumentSync: TextDocumentSyncKind.Full,
        definitionProvider: true,
        documentFormattingProvider: false,
        completionProvider: {
          resolveProvider: false,
          triggerCharacters: ['@', '"', '.'],
        },
        hoverProvider: true,
        renameProvider: false,
        documentSymbolProvider: true,
        codeLensProvider: {
          resolveProvider: true,
        },
        workspace: {
          fileOperations: {
            didCreate: {
              filters: [
                {
                  scheme: 'file',
                  pattern: {
                    glob: '**/*.{baml, json}',
                  },
                },
              ],
            },
            didDelete: {
              filters: [
                {
                  scheme: 'file',
                  pattern: {
                    glob: '**/*.{baml, json}',
                  },
                },
              ],
            },
            didRename: {
              filters: [
                {
                  scheme: 'file',
                  pattern: {
                    glob: '**/*.{baml, json}',
                  },
                },
              ],
            },
          },
        },
      },
    }

    const hasWorkspaceFolderCapability = !!(capabilities.workspace && !!capabilities.workspace.workspaceFolders)
    if (hasWorkspaceFolderCapability) {
      result.capabilities.workspace = {
        workspaceFolders: {
          supported: true,
        },
      }
    }

    // if (hasCodeActionLiteralsCapability) {
    //   result.capabilities.codeActionProvider = {
    //     codeActionKinds: [CodeActionKind.QuickFix],
    //   }
    // }

    return result
  })

  connection.onInitialized(() => {
    console.log('initialized')
    getConfig()

    if (hasConfigurationCapability) {
      // Register for all configuration changes.
      // eslint-disable-next-line @typescript-eslint/no-floating-promises
      connection.client.register(DidChangeConfigurationNotification.type)
      connection.client.register(DidChangeWatchedFilesNotification.type)
    }
  })

  // Cache the settings of all open documents
  const documentSettings: Map<string, Thenable<LSSettings>> = new Map<string, Thenable<LSSettings>>()

  const getConfig = async () => {
    try {
      const configResponse = await connection.workspace.getConfiguration('baml')
      console.log('configResponse ' + JSON.stringify(configResponse, null, 2))
      bamlConfig.config = bamlConfigSchema.parse(configResponse)
      await loadBamlCLIVersion()
    } catch (e: any) {
      if (e instanceof Error) {
        console.log('Error getting config' + e.message + ' ' + e.stack)
      } else {
        console.log('Error getting config' + e)
      }
    }
  }

  async function loadBamlCLIVersion(): Promise<void> {
    function parseVersion(input: string): string | null {
      const versionPattern = /(\d+\.\d+\.\d+)/
      const match = input.match(versionPattern)
      return match ? match[0] : null
    }

    if (bamlConfig.config?.cliPath) {
      const versionCommand = `${bamlConfig.config.cliPath} --version`
      try {
        const stdout = await new Promise<string>((resolve, reject) => {
          exec(versionCommand, (error, stdout, stderr) => {
            if (error) {
              reject(`Error running baml cli script: ${error}`)
            } else {
              resolve(stdout)
            }
          })
        })

        console.log(stdout)
        const version = parseVersion(stdout)
        if (version) {
          bamlConfig.cliVersion = version
        } else {
          throw new Error(`Error parsing baml cli version from output: ${stdout}`)
        }
      } catch (error) {
        console.error(error)
        connection.window.showErrorMessage(`BAML CLI Error: ${error}`)
      }
    } else {
      throw new Error('No CLI path found in config')
    }
  }

  function getLanguageExtension(uri: string): string | undefined {
    const languageExtension = uri.split('.').pop()
    if (!languageExtension) {
      console.log('Could not find language extension for ' + uri)
      return
    }
    return languageExtension
  }

  connection.onDidChangeWatchedFiles(async (params) => {
    // let deleted_files = params.changes.filter((change) =>
    //   change.type == FileChangeType.Deleted
    // ).map((change) => change.uri);
    // let created_files = params.changes.filter((change) =>
    //   change.type == FileChangeType.Created
    // ).map((change) => change.uri);
    // let changed_files = params.changes.filter((change) =>
    //   change.type == FileChangeType.Changed
    // ).map((change) => change.uri);

    // TODO: @hellovai be more efficient about this (only revalidate the files that changed)
    // If anything changes, then we need to revalidate all documents
    // let hasChanges = deleted_files.length > 0 || created_files.length > 0 || changed_files.length > 0;
    // console.log('watched files changed', params.changes)
    const hasChanges = params.changes.length > 0
    if (hasChanges) {
      // TODO: @hellovai we should technically get all possible root paths
      // (someone could delete mutliple baml_src dirs at once)
      await bamlProjectManager.reload_project_files(URI.parse(params.changes[0].uri))
    }
  })

  connection.onDidChangeConfiguration((_change) => {
    getConfig()
    if (hasConfigurationCapability) {
      // Reset all cached document settings
      documentSettings.clear()
    } else {
      // globalSettings = <LSSettings>(change.settings.prisma || defaultSettings) // eslint-disable-line @typescript-eslint/no-unsafe-member-access
    }

    // documents.all().forEach(debouncedValidateTextDocument) // eslint-disable-line @typescript-eslint/no-misused-promises
  })

  documents.onDidOpen(async (e) => {
    await bamlProjectManager.touch_project(URI.parse(e.document.uri))

    // e.document.uri
    // try {
    //   // TODO: revalidate if something changed
    //   bamlCache.addDocument(e.document)
    //   debouncedValidateTextDocument(e.document)
    // } catch (e: any) {
    //   if (e instanceof Error) {
    //     console.log('Error opening doc' + e.message + ' ' + e.stack)
    //   } else {
    //     console.log('Error opening doc' + e)
    //   }
    // }
  })

  // Note: VS Code strips newline characters from the message
  function showErrorToast(errorMessage: string): void {
    connection.window
      .showErrorMessage(errorMessage, {
        title: 'Show Details',
      })
      .then((item) => {
        if (item?.title === 'Show Details') {
          //  connection.sendNotification('baml/showLanguageServerOutput')
        }
      })
  }

  documents.onDidChangeContent(async (change: { document: TextDocument }) => {
    const textDocument = change.document

    await bamlProjectManager.upsert_file(URI.parse(textDocument.uri), textDocument.getText())
  })

  documents.onDidSave(async (change: { document: TextDocument }) => {
    const documentUri = URI.parse(change.document.uri)
    console.log('saving uri' + documentUri.toString() + '   ' + documentUri.fsPath)
    await bamlProjectManager.save_file(documentUri, change.document.getText())

    console.log('baml config ' + JSON.stringify(bamlConfig.config, null, 2))
    await loadBamlCLIVersion()

    if (bamlConfig.config?.generateCodeOnSave === 'always') {
      return
    }

    const proj = bamlProjectManager.getProjectById(documentUri)

    const error = proj.checkVersionOnSave()

    if (error) {
      connection.sendNotification('baml/message', {
        type: 'info',
        message: error,
        durationMs: 6000,
      })
    }

    try {
      if (bamlConfig.config?.cliPath) {
        cliBuild(
          bamlConfig.config?.cliPath!,
          URI.file(proj.rootPath()),
          (message) => {
            connection.window
              .showErrorMessage(message, {
                title: 'Show Details',
              })
              .then((item) => {
                if (item?.title === 'Show Details') {
                  connection.sendNotification('baml/showLanguageServerOutput')
                }
              })
          },
          () => {
            connection.sendNotification('baml/message', {
              type: 'info',
              message: 'BAML: Client generated! (Using installed baml-cli)',
            })
          },
        )
      } else {
        await bamlProjectManager.getProjectById(documentUri)?.runGeneratorsWithDebounce({
          onSuccess: (message: string) => connection.sendNotification('baml/message', { type: 'info', message }),
          onError: (message: string) => connection.sendNotification('baml/message', { type: 'error', message }),
        })
      }
    } catch (e) {
      console.error(`Error occurred while generating BAML client code:\n${e}`)
      showErrorToast(`Error occurred while generating BAML client code: ${e}`)
    }
  })

  function getDocument(uri: string): TextDocument | undefined {
    return documents.get(uri)
  }

  connection.onDefinition((params: DeclarationParams) => {
    const doc = getDocument(params.textDocument.uri)
    if (doc) {
      //accesses project from uri via bamlProjectManager
      const proj = bamlProjectManager.getProjectById(URI.parse(doc.uri))
      if (proj) {
        //returns the definition of reference within the project
        return proj.handleDefinitionRequest(doc, params.position)
      }
    }
    return undefined
  })

  connection.onCompletion((params: CompletionParams) => {
    try {
      const doc = getDocument(params.textDocument.uri)
      if (doc) {
        let completionWord = getWordAtPosition(doc, params.position)
        const splitWord = completionWord.split('{{')
        completionWord = splitWord[splitWord.length - 1]
        const proj = bamlProjectManager.getProjectById(URI.parse(doc.uri))
        const res = proj.verifyCompletionRequest(doc, params.position)
        if (res) {
          if (completionWord === '_.') {
            return {
              isIncomplete: false,
              items: [
                {
                  label: 'role("system")',
                },
                {
                  label: 'role("assistant")',
                },
                {
                  label: 'role("user")',
                },
              ],
            }
          } else if (completionWord === 'ctx.') {
            return {
              isIncomplete: false,
              items: [
                {
                  label: 'output_format',
                },
                {
                  label: 'client',
                },
              ],
            }
          } else if (completionWord === 'ctx.client.') {
            return {
              isIncomplete: false,
              items: [
                {
                  label: 'name',
                },
                {
                  label: 'provider',
                },
              ],
            }
          }
        }
      }
      return undefined
    } catch (e) {
      console.error(`Error occurred while generating completion:\n${e}`)
    }
  })
  // This handler resolves additional information for the item selected in the completion list.
  // connection.onCompletionResolve((completionItem: CompletionItem) => {
  //   return MessageHandler.handleCompletionResolveRequest(completionItem)
  // })

  connection.onHover((params: HoverParams) => {
    try {
      const doc = getDocument(params.textDocument.uri)
      if (doc) {
        const proj = bamlProjectManager.getProjectById(URI.parse(doc.uri))

        if (proj) {
          return proj.handleHoverRequest(doc, params.position)
        }
      }
      return undefined
    } catch (e) {
      console.error(`Error occurred while generating hover:\n${e}`)
    }
  })

  connection.onCodeLens((params: CodeLensParams) => {
    try {
      const document = getDocument(params.textDocument.uri)
      const codelenses = []

      if (document) {
        const proj = bamlProjectManager.getProjectById(URI.parse(document.uri))

        if (proj) {
          for (const func of proj.list_functions()) {
            if (URI.file(func.span.file_path).toString() === document.uri) {
              const range = Range.create(document.positionAt(func.span.start), document.positionAt(func.span.end))
              const command: Command = {
                title: '▶ Open Playground ✨',
                command: 'baml.openBamlPanel',
                arguments: [
                  {
                    projectId: proj,
                    functionName: func.name,
                    showTests: true,
                  },
                ],
              }
              codelenses.push({
                range,
                command,
              })
            }
          }
        }
      }
      return codelenses
    } catch (e) {
      console.error(`Error occurred while generating codelenses:\n${e}`)
    }
  })

  // connection.onDocumentFormatting((params: DocumentFormattingParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleDocumentFormatting(params, doc, showErrorToast)
  //   }
  // })

  // connection.onCodeAction((params: CodeActionParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleCodeActions(params, doc, showErrorToast)
  //   }
  // })

  // connection.onRenameRequest((params: RenameParams) => {
  //   const doc = getDocument(params.textDocument.uri)
  //   if (doc) {
  //     return MessageHandler.handleRenameRequest(params, doc)
  //   }
  // })

  connection.onDocumentSymbol((params: DocumentSymbolParams) => {
    return undefined
    // const doc = getDocument(params.textDocument.uri)
    // if (doc) {
    //   const db = bamlCache.getFileCache(doc)
    //   if (db) {
    //     let symbols = MessageHandler.handleDocumentSymbol(db, params, doc)
    //     return symbols
    //   }
    // }
  })

  connection.onRequest('requestDiagnostics', async () => {
    await bamlProjectManager.requestDiagnostics()
  })

  connection.onRequest(
    'selectTestCase',
    ({
      functionName,
      testCaseName,
    }: {
      functionName: string
      testCaseName: string
    }) => {
      // TODO deprecate
    },
  )

  connection.onRequest(
    'getDefinition',
    ({
      sourceFile,
      name,
    }: {
      sourceFile: string
      name: string
    }) => {
      // TODO deprecate
    },
  )

  connection.onRequest('cliVersion', async () => {
    // TODO deprecate
  })

  connection.onRequest('cliCheckForUpdates', async () => {
    // TODO deprecate
  })

  connection.onRequest(
    'getBAMLFunctions',
    async (): Promise<
      {
        name: string
        span: { file_path: string; start: number; end: number }
      }[]
    > => {
      const projects = bamlProjectManager.get_projects()

      const allFunctions = []
      for (const [id, proj] of projects.entries()) {
        const functions = proj.list_functions()
        allFunctions.push(
          ...functions.map(
            (
              func,
            ): {
              name: string
              span: { file_path: string; start: number; end: number }
            } => {
              return func.toJSON() as any
            },
          ),
        )
      }

      return allFunctions
    },
  )

  console.log('Server-side -- listening to connection')
  // Make the text document manager listen on the connection
  // for open, change and close text document events
  documents.listen(connection)

  connection.listen()
}
